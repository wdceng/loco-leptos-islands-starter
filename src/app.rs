use std::path::Path;

use async_trait::async_trait;
use axum::Router;
use leptos::config::get_configuration;
use loco_rs::{
    Error, Result,
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{BootResult, StartMode, create_app},
    config::Config,
    controller::{
        AppRoutes,
        middleware::{MiddlewareLayer, MiddlewareStackExt, default_middleware_stack},
    },
    db::{self, truncate_table},
    environment::Environment,
    task::Tasks,
};
use migration::Migrator;

use crate::{
    assets, controllers, deploy_checks, maintenance,
    middleware::{
        error_page::ErrorPages,
        rate_limit::{self, RateLimit},
    },
    models::_entities::users,
    settings::Settings,
    tasks,
    workers::downloader::DownloadWorker,
};

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment, config).await
    }

    /// One-time Leptos setup at boot.
    ///
    /// 1. Starts the task executor Leptos renders on. leptos_axum only does
    ///    this inside its own router helper, which we bypass because Loco
    ///    owns the routes. A second boot in the same process (tests) gets
    ///    `Err(already set)`, which is fine to ignore.
    /// 2. Loads the Leptos options and parks them in Loco's shared store,
    ///    where controllers pick them up to render pages. In development and
    ///    tests they come from `[package.metadata.leptos]` in Cargo.toml,
    ///    with any `LEPTOS_*` env vars (set by cargo-leptos) taking
    ///    precedence. In production there is no Cargo.toml next to the
    ///    binary, so the env vars alone are the source.
    /// 3. Parses the `settings:` block of the config into `Settings` and
    ///    parks it in the shared store too. A missing or malformed block
    ///    fails the boot here, before any middleware is built from it.
    /// 4. Builds the auth API's own token bucket from those settings and
    ///    parks it as well: `Hooks::routes` cannot fail, so the one step
    ///    that can (bad numbers, a key source that is unsafe when deployed)
    ///    happens here, where it refuses the boot.
    async fn after_context(ctx: AppContext) -> Result<AppContext> {
        let _ = any_spawner::Executor::init_tokio();

        let manifest = Path::new("Cargo.toml");
        let conf = get_configuration(manifest.exists().then_some("Cargo.toml"))
            .map_err(|e| Error::Message(format!("leptos configuration: {e}")))?;
        let mut options = conf.leptos_options;
        // Hashed asset names when the release build's hash file sits next
        // to the binary; switches `options.hash_files` on accordingly.
        let assets = assets::detect(&mut options, &ctx.environment)?;
        ctx.shared_store.insert(options);
        ctx.shared_store.insert(assets);
        ctx.shared_store.insert(Settings::from_config(&ctx.config)?);
        // A production secret that is set but empty or still `replace-me`
        // refuses the boot; a missing one `get_env` refused already.
        deploy_checks::refuse_placeholders(&ctx)?;

        // From the settings and the stylesheet now in the store.
        let limit = rate_limit::auth_bucket(&ctx)?;
        ctx.shared_store.insert(controllers::auth::AuthLimit(limit));
        Ok(ctx)
    }

    /// Once per server start, after the routes exist and the logger is up:
    /// schedules the nightly restart. Not in `after_context`: Loco's CLI
    /// runs that before the logger and `create_app` runs it again, so a
    /// spawn there would run twice and log nowhere. `routes`, `task` and the
    /// other CLI commands never reach this hook; the test harness does, with
    /// the restart off in `config/test.yaml` and refused in the test
    /// environment anyway.
    async fn after_routes(router: Router, ctx: &AppContext) -> Result<Router> {
        let settings: Settings = stored(ctx)?;
        maintenance::spawn(&settings.nightly_restart, &ctx.environment)?;
        // One SMTP login in the background when deployed: a wrong mail
        // account shows in the journal at boot, not at the first mail.
        deploy_checks::spawn_smtp_login(ctx);
        Ok(router)
    }

    /// Loco's default, config-driven stack with two changes. The router's
    /// fallback (the static files and the not-found page) is the project's
    /// own `site` in place of Loco's `static`, and first in the list: the
    /// stack is applied in order and each middleware wraps what the router
    /// holds by then, fallback included, so files and misses get the
    /// security headers, request id, request log, timeout and panic
    /// catching like routes. And `rate_limit`, inserted just outside
    /// `remote_ip`. Later in the list is further out on the request path,
    /// so the limiter runs inside `logger`, `request_id` and
    /// `secure_headers` (a 429 is logged with its request id and carries
    /// the security headers, which `tests/requests/rate_limit.rs` checks)
    /// and outside `etag`, `catch_panic` and `limit_payload` (a refused
    /// request never has its body read; nothing of ours there can panic).
    /// Loco's `etag` only answers `If-None-Match` for a response that
    /// already carries an ETag, and nothing in this app sets one.
    fn middlewares(ctx: &AppContext) -> Vec<Box<dyn MiddlewareLayer>> {
        let mut stack = default_middleware_stack(ctx);
        // Loco's `static` is replaced (its `static:` block in the config
        // stays the source of the folder and the cache header), and its
        // welcome-page `fallback` must never run: it would set a fallback of
        // its own over ours. The configs switch that one off too.
        stack.delete("static");
        stack.delete("fallback");
        stack.insert(0, Box::new(controllers::not_found::Site::from_context(ctx)));
        stack.insert_after("remote_ip", Box::new(RateLimit::from_context(ctx)));
        // After the timeout, so a 408 gets the page too; before
        // secure_headers and the logger, so the page gets their headers and
        // its own log line (middleware/error_page.rs).
        stack.insert_after("timeout_request", Box::new(ErrorPages::from_context(ctx)));
        stack
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![])
    }

    fn routes(ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes() // controller routes below
            .add_route(controllers::auth::routes(ctx))
            .add_route(controllers::home::routes())
            .add_route(controllers::robots::routes())
            .add_route(controllers::llms::routes())
            .add_route(controllers::manifest::routes())
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        Ok(())
    }

    fn register_tasks(tasks: &mut Tasks) {
        // tasks-inject (do not remove)
        tasks.register(tasks::user_create::UserCreate);
    }
    async fn truncate(ctx: &AppContext) -> Result<()> {
        truncate_table(&ctx.db, users::Entity).await?;
        Ok(())
    }
    async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
        db::seed::<users::ActiveModel>(&ctx.db, &base.join("users.yaml").display().to_string())
            .await?;
        Ok(())
    }
}

/// A value `after_context` parked in the shared store (the Leptos options,
/// `Assets`, `Settings`), cloned out.
///
/// # Errors
/// It is not there: the app did not boot through `after_context`.
pub fn stored<T: Clone + Send + Sync + 'static>(ctx: &AppContext) -> Result<T> {
    ctx.shared_store.get().ok_or_else(|| {
        Error::Message(format!(
            "{} missing from the shared store (after_context did not run)",
            std::any::type_name::<T>()
        ))
    })
}

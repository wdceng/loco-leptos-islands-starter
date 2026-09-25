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
use std::path::Path;

use crate::{
    assets, controllers, maintenance,
    middleware::rate_limit::{self, RateLimit},
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

        let manifest = std::path::Path::new("Cargo.toml");
        let conf = get_configuration(manifest.exists().then_some("Cargo.toml"))
            .map_err(|e| Error::Message(format!("leptos configuration: {e}")))?;
        let mut options = conf.leptos_options;
        // Hashed asset names when the release build's hash file sits next
        // to the binary; switches `options.hash_files` on accordingly.
        let assets = assets::detect(&mut options, &ctx.environment)?;
        ctx.shared_store.insert(options);
        ctx.shared_store.insert(assets);

        let settings = Settings::from_config(&ctx.config)?;
        let auth_limit = if settings.rate_limit.enable {
            let source = rate_limit::key_source(
                ctx.config.server.middlewares.remote_ip.as_ref(),
                &ctx.environment,
            )
            .map_err(Error::Message)?;
            let auth = &settings.rate_limit.auth;
            Some(rate_limit::bucket(auth.per_second, auth.burst, source)?)
        } else {
            None
        };
        ctx.shared_store
            .insert(controllers::auth::AuthLimit(auth_limit));
        ctx.shared_store.insert(settings);
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
        let settings: Settings = ctx
            .shared_store
            .get()
            .ok_or_else(|| Error::Message("settings missing from shared store".into()))?;
        maintenance::spawn(&settings.nightly_restart, &ctx.environment)?;
        Ok(router)
    }

    /// Loco's default, config-driven stack plus the project's own
    /// `rate_limit`, inserted just outside `remote_ip`. Later in the list is
    /// further out on the request path, so the limiter runs inside `logger`,
    /// `request_id` and `secure_headers` (a 429 is logged with its request id
    /// and carries the security headers) and outside `etag`, `catch_panic`
    /// and `limit_payload` (no ETag on a 429, nothing of ours to panic).
    fn middlewares(ctx: &AppContext) -> Vec<Box<dyn MiddlewareLayer>> {
        let mut stack = default_middleware_stack(ctx);
        stack.insert_after("remote_ip", Box::new(RateLimit::from_context(ctx)));
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

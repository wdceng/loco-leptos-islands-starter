//! `/robots.txt`, decided by environment. Only production may be indexed;
//! staging, development and test tell crawlers to stay away, so the staging
//! copy never competes with the live site in search results.

use loco_rs::{environment::Environment, prelude::*};

/// The file body for a given environment. Kept separate from the handler so
/// both branches can be unit-tested without booting the app.
pub fn body_for(env: &Environment) -> &'static str {
    match env {
        Environment::Production => "User-agent: *\nAllow: /\n",
        _ => "User-agent: *\nDisallow: /\n",
    }
}

#[debug_handler]
async fn robots(State(ctx): State<AppContext>) -> Result<Response> {
    format::text(body_for(&ctx.environment))
}

pub fn routes() -> Routes {
    Routes::new().add("/robots.txt", get(robots))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_is_open() {
        assert_eq!(
            body_for(&Environment::Production),
            "User-agent: *\nAllow: /\n"
        );
    }

    #[test]
    fn everything_else_is_closed() {
        for env in [
            Environment::Development,
            Environment::Test,
            Environment::Any("staging".into()),
        ] {
            assert_eq!(body_for(&env), "User-agent: *\nDisallow: /\n", "{env:?}");
        }
    }
}

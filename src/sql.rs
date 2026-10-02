//! The app's own queries run on SQLx (`sqlx::query!`, checked at compile
//! time), on the pool Sea-ORM opened for Loco: one set of connections, with
//! the PRAGMAs Loco applied. Sea-ORM keeps the migrations and Loco's own
//! models (`docs/architecture.md`, "Sea-ORM and SQLx").

use loco_rs::app::AppContext;
use sqlx::SqlitePool;

/// The SQLite pool behind `ctx.db`, for `sqlx::query!` and friends.
///
/// # Panics
/// Never in this app: Sea-ORM panics for a database other than SQLite, and
/// SQLite is the only driver compiled in (`Cargo.toml`).
#[must_use]
pub fn pool(ctx: &AppContext) -> &SqlitePool {
    ctx.db.get_sqlite_connection_pool()
}

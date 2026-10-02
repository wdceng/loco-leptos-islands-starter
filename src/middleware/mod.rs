//! The project's own Loco middlewares, added to the default stack in
//! `Hooks::middlewares` (src/app.rs). Server only.

pub mod error_page;
pub mod rate_limit;

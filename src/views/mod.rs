//! Two kinds of view live here. `auth` holds the JSON response shapes of
//! the `/api/auth` controller (Loco's convention). `layout` and the page
//! modules hold the Leptos components the page controllers render into HTML.

pub mod auth;
pub mod home;
pub mod layout;

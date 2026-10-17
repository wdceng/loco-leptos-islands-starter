# One job, one implementation

Every job the app does has one component, constant or helper. Something
that does an existing job is built from it, never from classes or code of
its own, and a change to how the job looks or works is made there, for
every use. Look a job up here before adding a component, a constant or a
helper. A new job gets a row when it's built.

Why: code written over many sessions drifts. Without this list the same
button, row or check gets written a second and a third time, each a little
different, and a change reaches only one of them.

## Pages

| Job | Use | Where |
|---|---|---|
| Render a page | `render_page` (CSP nonce, shell, status) | `src/render.rs` |
| The document around every page: head, header, footer, skip link | `shell` | `src/views/layout.rs` |
| A page's language, title, description, robots tag | `PageMeta` | `src/views/layout.rs` |
| The app's name, anywhere | `APP_NAME` | `src/views/layout.rs` |
| The site in one sentence (meta description, manifest, llms.txt) | `DESCRIPTION` | `src/controllers/home.rs` |
| The centred content column | `COLUMN` | `src/views/layout.rs` |
| The `<body>` classes | `BODY` | `src/views/layout.rs` |
| The not-found page | `NotFoundPage` | `src/views/not_found.rs` |
| An error page, and its words per status | `ErrorPage`, `wording` | `src/views/error.rs` |
| The 429 page, with the wait filled in | `too_many_requests::document`, `WAIT` | `src/views/too_many_requests.rs` |
| A password reset from the mail's link: the form, done, expired | `ResetForm`, `ResetDone`, `ResetExpired`; the address `reset::PATH` | `src/views/reset.rs`, `src/controllers/reset.rs` |
| A password's rule (8 to 128 characters), anywhere one is set | `password_problem` | `src/models/users.rs` |
| Hash or check a password | `hashing::hash`, `verify`, `verify_dummy` | `src/hashing.rs` |
| A page AI assistants should know about | a line in `/llms.txt` | `src/controllers/llms.rs` |

## Look

| Job | Use | Where |
|---|---|---|
| A colour in a view | the role tokens `primary`, `surface`, `ink`, `ink-muted`, never a scale step | `style/tailwind.css` |
| A colour where CSS can't reach (`theme-color`, the manifest) | `SURFACE_HEX`, `BRAND_HEX` | `src/views/layout.rs` |
| Room at the screen's edges in the home-screen app (a control under the clock or by the home indicator) | the `standalone:` variant, on a phone: `standalone:max-md:pt-6` | `style/tailwind.css` |

## Files and addresses

| Job | Use | Where |
|---|---|---|
| Link a file in `public/` | its constant in `paths::assets` (generated, versioned) | `src/paths.rs`, `build.rs` |
| The icons and the manifest | `paths::FAVICON*`, `APPLE_TOUCH_ICON`, `MANIFEST` | `src/paths.rs` |
| The stylesheet's address, hashed or plain | `Assets::stylesheet` | `src/assets.rs` |

## Browser

| Job | Use | Where |
|---|---|---|
| Show whether islands run (example lazy island) | `IslandsCheck` | `src/islands.rs` |

## Server

| Job | Use | Where |
|---|---|---|
| A query of the app's own | `sqlx::query!` on `sql::pool(&ctx)` | `src/sql.rs` |
| "Is this a developer's machine or the tests?" | `settings::is_local` | `src/settings.rs` |
| The app's typed settings | `Settings` | `src/settings.rs` |
| A rate-limit bucket | `site_bucket`, `auth_bucket`, `files_bucket` | `src/middleware/rate_limit.rs` |
| A visitor's address, behind the proxy or not | `VisitorIp`, `key_source` | `src/middleware/rate_limit.rs` |

## Mail

| Job | Use | Where |
|---|---|---|
| A mail's HTML part | a component in `views/mail.rs`, wrapped by `document` | `src/views/mail.rs` |
| Send a mail: sender, link origin, subject and text | `AuthMailer`'s `send` | `src/mailers/auth.rs` |

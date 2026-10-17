//! Password hashing, the one way: Argon2id through Loco's `hash`, run on
//! Tokio's blocking threads, at most four at a time.
//!
//! An Argon2id hash or check takes tens of milliseconds of CPU and about
//! 19 MB of memory. Run on the async threads, a burst of sign-ins would stall
//! every other request; run without a limit, it would take as much memory as
//! the burst asks for. Never call `loco_rs::hash` for a password directly.
//!
//! `verify_dummy` is for a sign-in whose email has no account: it checks the
//! password against a real hash made at startup by the same function, so the
//! answer takes as long as a real check and the time does not tell whether
//! the address is registered.

use std::sync::LazyLock;

use loco_rs::{Error, Result, hash};
use tokio::sync::Semaphore;

/// How many hashes or checks may run at once.
const AT_ONCE: usize = 4;

static PERMITS: Semaphore = Semaphore::const_new(AT_ONCE);

/// A hash of a value no account has, made with the same parameters as
/// every real one. `None` only if hashing itself fails, which the tests
/// rule out.
static DUMMY_HASH: LazyLock<Option<String>> =
    LazyLock::new(|| hash::hash_password("no account has this password").ok());

/// Runs `work` on a blocking thread, once one of the permits is free.
async fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    let _permit = PERMITS
        .acquire()
        .await
        .map_err(|e| Error::Message(format!("hashing: {e}")))?;
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Error::Message(format!("hashing: {e}")))
}

/// The Argon2id hash of `password`, for storing.
///
/// # Errors
/// Hashing failed, or its thread could not run.
pub async fn hash(password: &str) -> Result<String> {
    let password = password.to_owned();
    run(move || hash::hash_password(&password)).await?
}

/// Whether `password` matches the stored `hashed`. False when the hash is
/// malformed or its thread could not run.
pub async fn verify(password: &str, hashed: &str) -> bool {
    let (password, hashed) = (password.to_owned(), hashed.to_owned());
    run(move || hash::verify_password(&password, &hashed))
        .await
        .unwrap_or(false)
}

/// Builds the dummy hash now, at boot (`after_context`, app.rs), so the
/// first sign-in with an unknown email doesn't pay for it and stand out.
pub fn prepare() {
    LazyLock::force(&DUMMY_HASH);
}

/// A check that matches nothing and costs what [`verify`] costs: for a
/// sign-in whose email has no account.
pub async fn verify_dummy(password: &str) {
    let password = password.to_owned();
    let _ = run(move || {
        DUMMY_HASH
            .as_deref()
            .is_some_and(|dummy| hash::verify_password(&password, dummy))
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_hash_verifies_its_password_only() {
        let hashed = hash("correct horse").await.expect("hash");
        assert!(verify("correct horse", &hashed).await);
        assert!(!verify("wrong horse", &hashed).await);
        assert!(!verify("correct horse", "not a hash").await);
    }

    #[test]
    fn the_dummy_is_a_real_argon2id_hash() {
        let dummy = DUMMY_HASH.as_deref().expect("the dummy hash builds");
        assert!(dummy.starts_with("$argon2id$"), "{dummy}");
        // The same parameters as a stored hash, so a check costs the same.
        let real = hash::hash_password("x").expect("hash");
        let params = |h: &str| h.split('$').nth(3).map(str::to_owned);
        assert_eq!(params(dummy), params(&real));
    }
}

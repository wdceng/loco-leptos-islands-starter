//! Nightly restart: on staging and production the server stops itself once
//! a day and the service manager starts it again.
//!
//! A fresh process every day is cheap insurance against whatever a
//! long-running one accumulates. The numbers come from
//! `settings.nightly_restart` in `config/<env>.yaml` (`src/settings.rs`):
//! the hour of the day, and the IANA zone it is read in. The zone is fixed
//! rather than the server's local time, so the hour means what it says
//! whatever the box is set to.
//!
//! How it stops: SIGTERM to its own process, the signal `systemctl stop`
//! sends, so Loco's graceful shutdown runs: no new connections, requests in
//! flight finish, `on_shutdown` runs, exit status 0. If the signal cannot
//! be raised, or the platform has no signals, the process exits directly
//! instead. Either way the restart itself is the unit's `Restart=always`
//! (`DEPLOY.md`): without that line the stop is just a stop.
//!
//! Deployed environments only: `spawn` refuses to run in development, where
//! the stop would kill the `cargo leptos watch` server with nothing to
//! restart it, and in test, where the harness boots the app inside the test
//! process. The config files keep the block off locally as well; this is
//! the second lock.
//!
//! Daylight saving is handled: a time that does not exist on the
//! spring-forward night makes the loop wait and look again, and a time that
//! happens twice in autumn picks the later instance.

use chrono::{DateTime, Days, LocalResult, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use loco_rs::{Error, Result, environment::Environment};
use std::time::Duration;
use tracing::{info, warn};

use crate::settings::NightlyRestartSettings;

/// Starts the restart loop in the background and logs the decision either
/// way. Called once per server start, from `after_routes` in `app.rs`.
///
/// # Errors
/// `hour` is not an hour of the day. `Settings` validation rejects that at
/// boot, so a booted app never sees this error.
pub fn spawn(settings: &NightlyRestartSettings, env: &Environment) -> Result<()> {
    if !settings.enable {
        info!("nightly restart off");
        return Ok(());
    }
    if is_local(env) {
        warn!(
            "nightly restart is enabled in the {env} environment but only runs when deployed; ignoring"
        );
        return Ok(());
    }
    let at = settings.time().ok_or_else(|| {
        Error::Message(format!(
            "settings.nightly_restart.hour {} is not an hour of the day",
            settings.hour
        ))
    })?;
    let zone = settings.zone;
    info!(
        "nightly restart scheduled for {:02}:00 {zone}",
        settings.hour
    );
    tokio::spawn(run_nightly_restart_loop(at, zone));
    Ok(())
}

/// Development and test never restart, whatever the config says. Loco
/// parses an environment name it does not know into `Any`, which is how
/// `LOCO_ENV=staging` arrives here, so everything else counts as deployed.
fn is_local(env: &Environment) -> bool {
    matches!(env, Environment::Development | Environment::Test)
}

/// Sleeps until the next `at` in `zone`, then stops the process.
async fn run_nightly_restart_loop(at: NaiveTime, zone: Tz) {
    loop {
        let now = Utc::now().with_timezone(&zone);
        let naive = next_restart_naive(now.naive_local(), at);
        let Some(target) = resolve_local_dt(naive, &zone) else {
            // The spring-forward gap: tonight that wall-clock time does not
            // exist. Wait an hour and look again.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            continue;
        };
        let wait = (target - now).to_std().unwrap_or(Duration::from_secs(3600));
        info!("nightly restart at {target}");
        tokio::time::sleep(wait).await;

        // Loco logs to stdout synchronously, so this line reaches the
        // journal before the stop.
        info!("nightly restart: stopping, the service manager starts the service again");
        if stop_gracefully() {
            // Loco's shutdown is under way; this task has nothing left to do.
            return;
        }
        // No signals on this platform, or raising one failed: stop the hard
        // way. Status 0 is what `Restart=always` needs to bring it back.
        std::process::exit(0);
    }
}

/// Asks the process to stop the way `systemctl stop` would: SIGTERM to
/// itself, so Loco's graceful shutdown runs. Returns whether the signal
/// was raised.
#[cfg(unix)]
fn stop_gracefully() -> bool {
    use nix::sys::signal::{Signal, raise};

    match raise(Signal::SIGTERM) {
        Ok(()) => true,
        Err(e) => {
            warn!("nightly restart: could not raise SIGTERM ({e}), exiting instead");
            false
        }
    }
}

/// No signals to send here; the caller exits directly.
#[cfg(not(unix))]
fn stop_gracefully() -> bool {
    false
}

/// The next `at` strictly after `now`: today if it is still ahead, otherwise
/// tomorrow. Pure and zone-free.
fn next_restart_naive(now: NaiveDateTime, at: NaiveTime) -> NaiveDateTime {
    if now.time() >= at {
        (now.date() + Days::new(1)).and_time(at)
    } else {
        now.date().and_time(at)
    }
}

/// A wall-clock time in `tz`, with the daylight-saving edges decided: a
/// normal time is itself, a time that happens twice (autumn) is the later
/// instance, and a time that does not exist (spring) is `None`, so the
/// caller can wait and retry once the clock has moved past the gap.
fn resolve_local_dt<Z: TimeZone>(naive: NaiveDateTime, tz: &Z) -> Option<DateTime<Z>> {
    match tz.from_local_datetime(&naive) {
        LocalResult::Single(time) => Some(time),
        LocalResult::Ambiguous(_, later) => Some(later),
        LocalResult::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Offset;

    /// The zone of the daylight-saving fixtures below; the dates are its
    /// 2026 transitions.
    const ZONE: Tz = chrono_tz::Europe::Zagreb;

    fn at3() -> NaiveTime {
        NaiveTime::from_hms_opt(3, 0, 0).expect("03:00 is a time of day")
    }

    fn naive(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").expect("a valid test literal")
    }

    #[test]
    fn before_the_hour_restarts_today() {
        assert_eq!(
            next_restart_naive(naive("2026-04-30 00:30:00"), at3()),
            naive("2026-04-30 03:00:00")
        );
    }

    #[test]
    fn at_the_hour_restarts_tomorrow() {
        // The boundary: 03:00:00 itself is not "still ahead".
        assert_eq!(
            next_restart_naive(naive("2026-04-30 03:00:00"), at3()),
            naive("2026-05-01 03:00:00")
        );
    }

    #[test]
    fn after_the_hour_restarts_tomorrow() {
        assert_eq!(
            next_restart_naive(naive("2026-04-30 14:00:00"), at3()),
            naive("2026-05-01 03:00:00")
        );
    }

    #[test]
    fn normal_time_resolves_to_itself() {
        let n = naive("2026-06-15 12:00:00");
        let resolved = resolve_local_dt(n, &ZONE).expect("a normal time resolves");
        assert_eq!(resolved.naive_local(), n);
        assert_eq!(
            resolved.offset().fix().local_minus_utc(),
            2 * 3600,
            "CEST in June"
        );
    }

    #[test]
    fn spring_forward_gap_is_none() {
        // Zagreb, 2026-03-29: the clocks jump from 02:00 to 03:00, so 02:30
        // does not exist that night.
        assert!(resolve_local_dt(naive("2026-03-29 02:30:00"), &ZONE).is_none());
    }

    #[test]
    fn restart_time_exists_on_the_spring_forward_night() {
        // 03:00 is the first minute after the jump, so with the shipped
        // hour the retry branch in the loop is never needed; this pins that.
        let resolved = resolve_local_dt(naive("2026-03-29 03:00:00"), &ZONE).expect("03:00 exists");
        assert_eq!(
            resolved.offset().fix().local_minus_utc(),
            2 * 3600,
            "already CEST"
        );
    }

    #[test]
    fn fall_back_picks_the_later_instance() {
        // Zagreb, 2026-10-25: the clocks go from 03:00 back to 02:00, so
        // 02:30 happens twice, first in CEST (+02:00), then in CET (+01:00).
        let n = naive("2026-10-25 02:30:00");
        let resolved = resolve_local_dt(n, &ZONE).expect("an ambiguous time resolves");
        assert_eq!(resolved.naive_local(), n);
        assert_eq!(
            resolved.offset().fix().local_minus_utc(),
            3600,
            "the later one is CET"
        );
    }

    #[test]
    fn local_environments_never_restart() {
        assert!(is_local(&Environment::Development));
        assert!(is_local(&Environment::Test));
        assert!(!is_local(&Environment::Production));
        assert!(!is_local(&Environment::Any("staging".to_string())));
    }
}

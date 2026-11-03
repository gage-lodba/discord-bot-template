//! Tracks members removed by moderation, so the leave announcement is not sent
//! for someone who was kicked or banned.
//!
//! The problem this solves: Discord fires `GUILD_MEMBER_REMOVE` for a voluntary
//! leave, a kick *and* a ban, and the payload does not say which. The audit log
//! entry that does say which is written by Discord server-side, so it reliably
//! arrives *after* the removal event. Reading the audit log at that moment
//! therefore races and misses.
//!
//! So `guild_member_removal` waits [`RECONCILE_DELAY`] for the audit entry to
//! land, then asks this module whether the removal was a moderation action.
//! The delay is the cost of knowing; without it the leave message has already
//! been sent by the time the kick entry arrives.
//!
//! Reference: `docs/gateway-events.md` (Guild Member Remove, Guild Audit Log
//! Entry Create).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use discord::id::UserId;

use tracing::debug;

/// How long a leave announcement is held back while a moderation audit entry
/// may still be in flight.
///
/// Genuine leaves are announced after this delay. Raise it if kicks are still
/// double-posting in a slow or heavily rate-limited environment; lower it only
/// if the delay on real leaves matters more than the duplicates.
pub(crate) const RECONCILE_DELAY: Duration = Duration::from_millis(1500);

/// How long a recorded moderation action keeps suppressing leave announcements.
///
/// Must exceed [`RECONCILE_DELAY`] so an entry that lands during the wait is
/// still seen, and stays generous because audit entries can be delayed.
const MODERATION_TTL: Duration = Duration::from_secs(10);

/// Moderation actions that should replace the leave announcement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Moderation {
    Kick,
    Ban,
}

fn recent() -> &'static Mutex<HashMap<UserId, (Moderation, Instant)>> {
    static RECENT: OnceLock<Mutex<HashMap<UserId, (Moderation, Instant)>>> = OnceLock::new();

    RECENT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records that a member was removed by moderation.
///
/// Called from the audit-log handler for kick and ban entries.
pub(crate) fn record(user_id: UserId, action: Moderation) {
    let Ok(mut map) = recent().lock() else {
        // A poisoned mutex means another thread panicked while holding it. Losing
        // the record only risks a duplicate announcement, so this is not fatal.
        debug!("moderation tracker lock poisoned while recording");
        return;
    };

    let now = Instant::now();
    map.retain(|_, (_, at)| now.duration_since(*at) < MODERATION_TTL);
    map.insert(user_id, (action, now));
}

/// Returns the moderation action recorded for `user_id`, if it is recent.
pub(crate) fn action_for(user_id: UserId) -> Option<Moderation> {
    let mut map = recent().lock().ok()?;

    let now = Instant::now();
    map.retain(|_, (_, at)| now.duration_since(*at) < MODERATION_TTL);

    map.get(&user_id).map(|(action, _)| *action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_returns_the_action() {
        let user = UserId::new(11);

        record(user, Moderation::Kick);

        assert_eq!(action_for(user), Some(Moderation::Kick));
    }

    #[test]
    fn a_later_action_replaces_the_earlier_one() {
        let user = UserId::new(12);

        record(user, Moderation::Kick);
        record(user, Moderation::Ban);

        assert_eq!(action_for(user), Some(Moderation::Ban));
    }

    #[test]
    fn an_unrecorded_member_has_no_action() {
        assert_eq!(action_for(UserId::new(13)), None);
    }

    #[test]
    fn a_stale_record_is_not_returned() {
        let user = UserId::new(14);

        // Insert directly with a timestamp older than the TTL, so the test does
        // not have to sleep.
        {
            let mut map = recent().lock().expect("lock");
            map.insert(
                user,
                (
                    Moderation::Kick,
                    Instant::now()
                        .checked_sub(MODERATION_TTL + Duration::from_secs(1))
                        .expect("the test process has run for longer than the moderation TTL"),
                ),
            );
        }

        assert_eq!(action_for(user), None);
    }

    #[test]
    fn the_ttl_outlasts_the_reconcile_delay() {
        // Otherwise an audit entry that lands during the wait could already have
        // expired by the time the leave handler asks about it.
        assert!(MODERATION_TTL > RECONCILE_DELAY);
    }
}

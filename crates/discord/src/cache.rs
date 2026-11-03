//! The small in-memory cache the bot needs: the guilds, their members, the
//! current user and the application id.
//!
//! Reads are lock-free from the caller's point of view: every getter returns an
//! owned value (mostly an [`Arc`] clone) so no lock guard ever escapes the
//! cache. Writes take a short-lived write lock. A poisoned lock is logged and
//! recovered from rather than turned into a panic — a handler that panics must
//! not take the whole bot down.
//!
//! Reference: `docs/gateway.md` (Tracking State, Guild Availability),
//! `docs/gateway-events.md` (Ready, Guild Create, Guild Member Add/Remove).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use tracing::warn;

use crate::id::{ApplicationId, GuildId, UserId};
use crate::model::guild::{Guild, Member};
use crate::model::user::User;

/// The guild / user state the gateway updates and the bot reads.
///
/// Only available guilds are stored; while a guild is unavailable (the state
/// every guild starts in after Ready) its id is remembered separately so a
/// `GUILD_CREATE` can be recognised later. Reference: `docs/gateway.md` (Guild
/// Availability).
#[derive(Debug, Default)]
pub struct Cache {
    inner: RwLock<CacheInner>,
}

/// The value behind the cache's lock.
#[derive(Debug, Default)]
struct CacheInner {
    current_user: Option<Arc<User>>,
    guilds: HashMap<GuildId, Arc<Guild>>,
    unavailable_guilds: HashSet<GuildId>,
    application_id: Option<ApplicationId>,
}

impl Cache {
    /// Creates an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the bot user from the Ready event, once it has been received.
    ///
    /// Reference: `docs/gateway-events.md` (Ready).
    #[must_use]
    pub fn current_user(&self) -> Option<Arc<User>> {
        self.read().current_user.clone()
    }

    /// Returns a cached, available guild.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Create).
    #[must_use]
    pub fn guild(&self, guild_id: GuildId) -> Option<Arc<Guild>> {
        self.read().guilds.get(&guild_id).cloned()
    }

    /// Returns every cached, available guild.
    ///
    /// The order is unspecified.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Create).
    #[must_use]
    pub fn guilds(&self) -> Vec<Arc<Guild>> {
        self.read().guilds.values().cloned().collect()
    }

    /// Returns the application (bot) id from the Ready event, once received.
    ///
    /// Reference: `docs/gateway-events.md` (Ready).
    #[must_use]
    pub fn application_id(&self) -> Option<ApplicationId> {
        self.read().application_id
    }

    /// Stores the bot user from Ready.
    pub(crate) fn set_current_user(&self, user: User) {
        self.write().current_user = Some(Arc::new(user));
    }

    /// Stores the application id from Ready.
    pub(crate) fn set_application_id(&self, application_id: ApplicationId) {
        self.write().application_id = Some(application_id);
    }

    /// Records the guilds Ready listed as unavailable.
    ///
    /// Reference: `docs/gateway-events.md` (Ready — guilds are unavailable until
    /// their Guild Create arrives).
    pub(crate) fn set_unavailable_guilds(&self, guild_ids: impl IntoIterator<Item = GuildId>) {
        let mut inner = self.write();
        inner.unavailable_guilds = guild_ids.into_iter().collect();
        // A guild that was already cached must not be considered unavailable.
        let cached: Vec<GuildId> = inner.guilds.keys().copied().collect();
        for guild_id in cached {
            inner.unavailable_guilds.remove(&guild_id);
        }
    }

    /// Marks one guild as unavailable.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Create — unavailable guild
    /// object).
    pub(crate) fn mark_guild_unavailable(&self, guild_id: GuildId) {
        self.write().unavailable_guilds.insert(guild_id);
    }

    /// Returns true when the guild is known but not yet available.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn is_guild_unavailable(&self, guild_id: GuildId) -> bool {
        self.read().unavailable_guilds.contains(&guild_id)
    }

    /// Stores an available guild, replacing any previous copy.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Create).
    pub(crate) fn insert_guild(&self, guild: Guild) {
        let guild_id = guild.id;
        let mut inner = self.write();
        inner.unavailable_guilds.remove(&guild_id);
        inner.guilds.insert(guild_id, Arc::new(guild));
    }

    /// Adds or replaces a member in a cached guild.
    ///
    /// Guilds are cached behind [`Arc`], so the guild is cloned and replaced.
    /// When the guild is not cached yet the member is dropped with a debug log;
    /// the Guild Create that follows carries the member list anyway.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Member Add).
    pub(crate) fn update_member(&self, guild_id: GuildId, member: Member) {
        let mut inner = self.write();
        let Some(guild) = inner.guilds.get(&guild_id) else {
            tracing::debug!(%guild_id, "ignoring a member update for an uncached guild");
            return;
        };

        let mut updated = Guild::clone(guild);
        updated.members.insert(member.user.id, member);
        inner.guilds.insert(guild_id, Arc::new(updated));
    }

    /// Removes a member from a cached guild, if both are present.
    ///
    /// Reference: `docs/gateway-events.md` (Guild Member Remove).
    pub(crate) fn remove_member(&self, guild_id: GuildId, user_id: UserId) {
        let mut inner = self.write();
        let Some(guild) = inner.guilds.get(&guild_id) else {
            return;
        };

        if !guild.members.contains_key(&user_id) {
            return;
        }

        let mut updated = Guild::clone(guild);
        updated.members.remove(&user_id);
        inner.guilds.insert(guild_id, Arc::new(updated));
    }

    /// Locks the cache for reading, recovering from a poisoned lock.
    fn read(&self) -> RwLockReadGuard<'_, CacheInner> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("the cache read lock was poisoned; continuing with the poisoned state");
                poisoned.into_inner()
            }
        }
    }

    /// Locks the cache for writing, recovering from a poisoned lock.
    fn write(&self) -> RwLockWriteGuard<'_, CacheInner> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("the cache write lock was poisoned; continuing with the poisoned state");
                poisoned.into_inner()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Builds a guild the way the gateway sends it: arrays for roles, channels
    /// and members, everything else as documented in `docs/guild.md`.
    fn guild(id: u64, name: &str) -> Guild {
        serde_json::from_value(json!({
            "id": id.to_string(),
            "name": name,
            "icon": null,
            "owner_id": "1",
            "roles": [],
            "channels": [],
            "threads": [],
            "members": [],
            "features": [],
            "member_count": 1,
        }))
        .expect("documented guild payload")
    }

    fn user(id: u64) -> User {
        serde_json::from_value(json!({
            "id": id.to_string(),
            "username": "tester",
            "discriminator": "0",
            "global_name": null,
            "avatar": null,
            "bot": false,
            "system": false,
            "flags": null,
            "accent_color": null,
            "banner": null,
        }))
        .expect("documented user payload")
    }

    fn member(id: u64) -> Member {
        serde_json::from_value(json!({
            "user": {
                "id": id.to_string(),
                "username": "tester",
                "discriminator": "0",
                "bot": false,
                "system": false,
            },
            "nick": null,
            "roles": [],
            "joined_at": null,
            "deaf": false,
            "mute": false,
            "pending": null,
        }))
        .expect("documented member payload")
    }

    #[test]
    fn new_cache_is_empty() {
        let cache = Cache::new();

        assert!(cache.current_user().is_none());
        assert!(cache.application_id().is_none());
        assert!(cache.guilds().is_empty());
        assert!(cache.guild(GuildId::new(1)).is_none());
    }

    #[test]
    fn current_user_and_application_id_are_cached() {
        let cache = Cache::new();

        cache.set_current_user(user(2));
        cache.set_application_id(ApplicationId::new(3));

        assert_eq!(
            cache.current_user().map(|user| user.id),
            Some(UserId::new(2))
        );
        assert_eq!(cache.application_id(), Some(ApplicationId::new(3)));
    }

    #[test]
    fn guilds_are_cached_and_unavailable_marking_is_cleared() {
        let cache = Cache::new();
        let guild_id = GuildId::new(10);

        cache.set_unavailable_guilds([guild_id, GuildId::new(11)]);
        assert!(cache.is_guild_unavailable(guild_id));

        cache.insert_guild(guild(10, "Nocturne"));

        assert!(!cache.is_guild_unavailable(guild_id));
        assert!(cache.is_guild_unavailable(GuildId::new(11)));
        assert_eq!(
            cache.guild(guild_id).map(|guild| guild.name.clone()),
            Some("Nocturne".to_owned())
        );
        assert_eq!(cache.guilds().len(), 1);
    }

    #[test]
    fn members_are_added_to_and_removed_from_cached_guilds() {
        let cache = Cache::new();
        let guild_id = GuildId::new(10);
        cache.insert_guild(guild(10, "Nocturne"));

        cache.update_member(guild_id, member(20));
        assert!(
            cache
                .guild(guild_id)
                .is_some_and(|guild| guild.members.contains_key(&UserId::new(20)))
        );

        cache.remove_member(guild_id, UserId::new(20));
        assert!(
            cache
                .guild(guild_id)
                .is_some_and(|guild| !guild.members.contains_key(&UserId::new(20)))
        );
    }

    #[test]
    fn member_updates_for_an_unknown_guild_are_ignored() {
        let cache = Cache::new();

        cache.update_member(GuildId::new(10), member(20));
        cache.remove_member(GuildId::new(10), UserId::new(20));

        assert!(cache.guilds().is_empty());
    }

    #[test]
    fn a_poisoned_cache_lock_does_not_panic() {
        let cache = Arc::new(Cache::new());

        let poisoned = Arc::clone(&cache);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.inner.write().expect("lock is healthy");
            panic!("poison the cache lock");
        })
        .join();

        assert!(cache.current_user().is_none());
        assert!(cache.guilds().is_empty());
        cache.set_current_user(user(2));
        assert!(cache.current_user().is_some());
    }
}

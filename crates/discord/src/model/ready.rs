//! Gateway connection models: the `READY` and `RESUMED` dispatches.
//!
//! Reference: `docs/gateway-events.md` (Ready, Resumed).

use serde::Deserialize;
use serde::Deserializer;

use crate::id::{ApplicationId, GuildId};
use crate::model::user::User;

/// The `READY` dispatch, sent once the initial gateway handshake completes.
///
/// Reference: `docs/gateway-events.md` (Ready Event Fields).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Ready {
    /// Information about the connected user.
    pub user: User,
    /// The guilds the user is in; they start unavailable and arrive as
    /// `GUILD_CREATE` events.
    #[serde(default)]
    pub guilds: Vec<UnavailableGuild>,
    /// The session id used for resuming connections.
    pub session_id: String,
    /// The gateway URL for resuming connections.
    pub resume_gateway_url: String,
    /// The shard information `(shard_id, num_shards)` associated with this
    /// session, if the client identified with one.
    #[serde(default)]
    pub shard: Option<(i64, i64)>,
    /// The application this bot belongs to.
    pub application: PartialApplication,
}

/// The partial guild object sent in `READY` and `GUILD_DELETE`.
///
/// Reference: `docs/guild.md` (Unavailable Guild Object) and
/// `docs/gateway-events.md` (Ready, Guild Delete).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UnavailableGuild {
    /// The guild id.
    pub id: GuildId,
    /// Whether the guild is unavailable. `None` on a `GUILD_DELETE` means the
    /// user was removed from the guild.
    #[serde(default)]
    pub unavailable: Option<bool>,
}

/// The partial application object sent in `READY`.
///
/// Reference: `docs/gateway-events.md` (Ready) and `docs/application.md`
/// (Application Object).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PartialApplication {
    /// The application (bot) id.
    pub id: ApplicationId,
    /// The application's public flags (`docs/application.md`, Application
    /// Flags).
    #[serde(default)]
    pub flags: Option<u64>,
}

/// The `RESUMED` dispatch, sent after a successful resume.
///
/// The event carries no fields, so its deserialiser accepts any payload shape
/// (`null`, `{}` or an object with future fields).
///
/// Reference: `docs/gateway-events.md` (Resumed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumed;

impl<'de> Deserialize<'de> for Resumed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Deliberately accept and discard whatever the gateway sent; RESUMED
        // is defined to have no payload.
        let _ = serde_json::Value::deserialize(deserializer)?;

        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::UserId;

    #[test]
    fn parses_the_documented_ready_event() {
        let ready: Ready = serde_json::from_value(serde_json::json!({
            "v": 10,
            "user": {
                "id": "123456789012345678",
                "username": "bot",
                "discriminator": "0",
                "global_name": null,
                "avatar": null,
                "bot": true
            },
            "guilds": [
                {"id": "1", "unavailable": true},
                {"id": "2", "unavailable": true}
            ],
            "session_id": "session-id",
            "resume_gateway_url": "wss://gateway-us-east1-b.discord.gg",
            "shard": [0, 1],
            "application": {
                "id": "999",
                "flags": 0,
                "flags_new": "0"
            }
        }))
        .expect("documented ready payload");

        assert_eq!(ready.user.id, UserId::new(123_456_789_012_345_678));
        assert!(ready.user.bot);
        assert_eq!(ready.guilds.len(), 2);
        assert_eq!(ready.guilds[0].id, GuildId::new(1));
        assert_eq!(ready.guilds[0].unavailable, Some(true));
        assert_eq!(ready.session_id, "session-id");
        assert_eq!(
            ready.resume_gateway_url,
            "wss://gateway-us-east1-b.discord.gg"
        );
        assert_eq!(ready.shard, Some((0, 1)));
        assert_eq!(ready.application.id, ApplicationId::new(999));
        assert_eq!(ready.application.flags, Some(0));
    }

    #[test]
    fn ready_without_a_shard_or_updated_url_parses() {
        let ready: Ready = serde_json::from_value(serde_json::json!({
            "user": {"id": "1", "username": "bot"},
            "guilds": [{"id": "2"}],
            "session_id": "s",
            "resume_gateway_url": "wss://gateway.discord.gg",
            "application": {"id": "3"}
        }))
        .expect("ready payload without shard");

        assert_eq!(ready.shard, None);
        assert_eq!(ready.guilds[0].unavailable, None);
        assert_eq!(ready.application.flags, None);
    }

    #[test]
    fn unavailable_guild_parses_from_the_documented_example() {
        let guild: UnavailableGuild = serde_json::from_value(serde_json::json!({
            "id": "41771983423143937",
            "unavailable": true
        }))
        .expect("unavailable guild payload");

        assert_eq!(guild.id, GuildId::new(41_771_983_423_143_937));
        assert_eq!(guild.unavailable, Some(true));

        let removed: UnavailableGuild =
            serde_json::from_value(serde_json::json!({"id": "1"})).expect("guild delete payload");
        assert_eq!(removed.unavailable, None);
    }

    #[test]
    fn resumed_accepts_any_payload() {
        assert_eq!(
            serde_json::from_value::<Resumed>(serde_json::json!(null)).expect("null"),
            Resumed
        );
        assert_eq!(
            serde_json::from_value::<Resumed>(serde_json::json!({})).expect("empty object"),
            Resumed
        );
        assert_eq!(
            serde_json::from_value::<Resumed>(serde_json::json!({"future": true}))
                .expect("object with fields"),
            Resumed
        );
    }
}

//! Channel models, including permission overwrites and channel kinds.
//!
//! Reference: `docs/channel.md` (Channel Object, Overwrite Object) and
//! `docs/permissions.md` (Permission Overwrites).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::{de::Error as DeError, de::Visitor};
use std::fmt;

use crate::id::{ChannelId, GenericId, GuildId};
use crate::permissions::Permissions;

/// The kind of a channel, the integer `type` field of the channel object.
///
/// Reference: `docs/channel.md` (Channel Types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    /// Type `0`: a text channel within a server.
    GuildText,
    /// Type `1`: a direct message between users.
    Dm,
    /// Type `2`: a voice channel within a server.
    GuildVoice,
    /// Type `3`: a direct message between multiple users.
    GroupDm,
    /// Type `4`: an organizational category.
    GuildCategory,
    /// Type `5`: a channel users can follow and crosspost into their own guild.
    GuildAnnouncement,
    /// Type `10`: a temporary sub-channel within an announcement channel.
    AnnouncementThread,
    /// Type `11`: a temporary sub-channel within a text or forum channel.
    PublicThread,
    /// Type `12`: a temporary sub-channel viewable only by invitees and members
    /// with `MANAGE_THREADS`.
    PrivateThread,
    /// Type `13`: a voice channel for hosting events with an audience.
    GuildStageVoice,
    /// Type `14`: the channel in a hub containing the listed servers.
    GuildDirectory,
    /// Type `15`: a channel that can only contain threads.
    GuildForum,
    /// Type `16`: a forum-like channel that can only contain threads.
    GuildMedia,
    /// A channel type this crate does not model, kept so a new Discord channel
    /// kind cannot break deserialisation.
    Unknown(i64),
}

impl ChannelKind {
    /// Maps the documented integer `type` value to a [`ChannelKind`].
    ///
    /// Reference: `docs/channel.md` (Channel Types).
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            0 => Self::GuildText,
            1 => Self::Dm,
            2 => Self::GuildVoice,
            3 => Self::GroupDm,
            4 => Self::GuildCategory,
            5 => Self::GuildAnnouncement,
            10 => Self::AnnouncementThread,
            11 => Self::PublicThread,
            12 => Self::PrivateThread,
            13 => Self::GuildStageVoice,
            14 => Self::GuildDirectory,
            15 => Self::GuildForum,
            16 => Self::GuildMedia,
            other => Self::Unknown(other),
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::GuildText => 0,
            Self::Dm => 1,
            Self::GuildVoice => 2,
            Self::GroupDm => 3,
            Self::GuildCategory => 4,
            Self::GuildAnnouncement => 5,
            Self::AnnouncementThread => 10,
            Self::PublicThread => 11,
            Self::PrivateThread => 12,
            Self::GuildStageVoice => 13,
            Self::GuildDirectory => 14,
            Self::GuildForum => 15,
            Self::GuildMedia => 16,
            Self::Unknown(value) => value,
        }
    }

    /// Returns whether this channel kind is a thread.
    ///
    /// Threads are `ANNOUNCEMENT_THREAD` (10), `PUBLIC_THREAD` (11) and
    /// `PRIVATE_THREAD` (12).
    ///
    /// Reference: `docs/channel.md` (Channel Types).
    #[must_use]
    pub const fn is_thread(self) -> bool {
        matches!(
            self,
            Self::AnnouncementThread | Self::PublicThread | Self::PrivateThread
        )
    }
}

impl Serialize for ChannelKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for ChannelKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = ChannelKind;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a channel type integer")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                Ok(ChannelKind::from_i64(value))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .map(ChannelKind::from_i64)
                    .map_err(|_| E::custom("channel type out of range"))
            }
        }

        deserializer.deserialize_i64(KindVisitor)
    }
}

/// Whether a permission overwrite applies to a role or to a single member.
///
/// Reference: `docs/channel.md` (Overwrite Object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverwriteKind {
    /// Type `0`: the overwrite id is a role id.
    Role,
    /// Type `1`: the overwrite id is a user id.
    Member,
}

impl OverwriteKind {
    /// Maps the documented integer `type` value to an [`OverwriteKind`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Member,
            _ => Self::Role,
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Role => 0,
            Self::Member => 1,
        }
    }
}

impl Serialize for OverwriteKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for OverwriteKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = OverwriteKind;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an overwrite type integer (0 for role, 1 for member)")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                Ok(OverwriteKind::from_i64(value))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .map(OverwriteKind::from_i64)
                    .map_err(|_| E::custom("overwrite type out of range"))
            }
        }

        deserializer.deserialize_i64(KindVisitor)
    }
}

/// A per-channel permission overwrite for a role or a member.
///
/// Reference: `docs/channel.md` (Overwrite Object) and `docs/permissions.md`
/// (Permission Overwrites).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOverwrite {
    /// The role id or user id this overwrite applies to, depending on
    /// [`PermissionOverwrite::kind`].
    pub id: GenericId,
    /// Whether the id names a role or a member.
    #[serde(rename = "type")]
    pub kind: OverwriteKind,
    /// Permissions explicitly allowed in this channel.
    pub allow: Permissions,
    /// Permissions explicitly denied in this channel.
    pub deny: Permissions,
}

/// A guild channel or direct-message channel.
///
/// Only the fields this crate consumes are modelled; everything else Discord
/// sends is ignored by serde. Optional and nullable fields are [`Option`]s.
///
/// Reference: `docs/channel.md` (Channel Structure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    /// The id of this channel.
    pub id: ChannelId,
    /// The kind of channel (`type` on the wire).
    #[serde(rename = "type")]
    pub kind: ChannelKind,
    /// The id of the guild this channel belongs to, when it is a guild channel.
    /// May be missing for some channel objects received over gateway guild
    /// dispatches.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
    /// The parent category id, or for a thread the id of the channel it was
    /// created in.
    #[serde(default)]
    pub parent_id: Option<ChannelId>,
    /// The name of the channel, when it has one.
    #[serde(default)]
    pub name: Option<String>,
    /// The sorting position of the channel.
    #[serde(default)]
    pub position: Option<i64>,
    /// Explicit permission overwrites for members and roles.
    #[serde(default)]
    pub permission_overwrites: Vec<PermissionOverwrite>,
    /// The channel topic.
    #[serde(default)]
    pub topic: Option<String>,
    /// Whether the channel is age-restricted.
    #[serde(default)]
    pub nsfw: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_guild_text_channel() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": "41771983423143937",
            "guild_id": "41771983423143937",
            "name": "general",
            "type": 0,
            "position": 6,
            "permission_overwrites": [],
            "rate_limit_per_user": 2,
            "nsfw": true,
            "topic": "24/7 chat about how to gank Mike #2",
            "last_message_id": "155117677105512449",
            "parent_id": "399942396007890945",
            "default_auto_archive_duration": 60
        }))
        .expect("documented channel payload");

        assert_eq!(channel.kind, ChannelKind::GuildText);
        assert_eq!(channel.name.as_deref(), Some("general"));
        assert_eq!(channel.position, Some(6));
        assert_eq!(channel.nsfw, Some(true));
        assert_eq!(
            channel.topic.as_deref(),
            Some("24/7 chat about how to gank Mike #2")
        );
        assert_eq!(
            channel.parent_id,
            Some(ChannelId::new(399_942_396_007_890_945))
        );
        assert!(channel.permission_overwrites.is_empty());
        assert!(!channel.kind.is_thread());
    }

    #[test]
    fn parses_the_documented_category_channel() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "permission_overwrites": [],
            "name": "Test",
            "parent_id": null,
            "nsfw": false,
            "position": 0,
            "guild_id": "290926798629997250",
            "type": 4,
            "id": "399942396007890945"
        }))
        .expect("documented category payload");

        assert_eq!(channel.kind, ChannelKind::GuildCategory);
        assert_eq!(channel.parent_id, None);
        assert_eq!(channel.nsfw, Some(false));
    }

    #[test]
    fn parses_the_documented_thread_channel() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": "41771983423143937",
            "guild_id": "41771983423143937",
            "parent_id": "41771983423143937",
            "owner_id": "41771983423143937",
            "name": "don't buy dota-2",
            "type": 11,
            "last_message_id": "155117677105512449",
            "message_count": 1,
            "member_count": 5,
            "rate_limit_per_user": 2,
            "thread_metadata": {
                "archived": false,
                "auto_archive_duration": 1440,
                "archive_timestamp": "2021-04-12T23:40:39.855793+00:00",
                "locked": false
            },
            "total_message_sent": 1
        }))
        .expect("documented thread payload");

        assert_eq!(channel.kind, ChannelKind::PublicThread);
        assert!(channel.kind.is_thread());
        assert_eq!(
            channel.parent_id,
            Some(ChannelId::new(41_771_983_423_143_937))
        );
    }

    #[test]
    fn parses_a_dm_channel_without_guild_id() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "last_message_id": "3343820033257021450",
            "type": 1,
            "id": "319674150115610528",
            "recipients": [
                {
                    "username": "test",
                    "discriminator": "9999",
                    "id": "82198898841029460",
                    "avatar": "33ecab261d4681afa4d85a04691c4a01"
                }
            ]
        }))
        .expect("documented DM payload");

        assert_eq!(channel.kind, ChannelKind::Dm);
        assert_eq!(channel.guild_id, None);
        assert_eq!(channel.name, None);
    }

    #[test]
    fn parses_permission_overwrites() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": "1",
            "type": 0,
            "permission_overwrites": [
                {
                    "id": "10",
                    "type": 0,
                    "allow": "2048",
                    "deny": "0"
                },
                {
                    "id": "20",
                    "type": 1,
                    "allow": "0",
                    "deny": "1024"
                }
            ]
        }))
        .expect("channel with overwrites");

        assert_eq!(channel.permission_overwrites.len(), 2);
        assert_eq!(channel.permission_overwrites[0].kind, OverwriteKind::Role);
        assert_eq!(
            channel.permission_overwrites[0].allow,
            Permissions::SEND_MESSAGES
        );
        assert_eq!(channel.permission_overwrites[1].kind, OverwriteKind::Member);
        assert_eq!(
            channel.permission_overwrites[1].deny,
            Permissions::VIEW_CHANNEL
        );
    }

    #[test]
    fn channel_kinds_round_trip_the_documented_integers() {
        for (value, kind) in [
            (0, ChannelKind::GuildText),
            (1, ChannelKind::Dm),
            (2, ChannelKind::GuildVoice),
            (3, ChannelKind::GroupDm),
            (4, ChannelKind::GuildCategory),
            (5, ChannelKind::GuildAnnouncement),
            (10, ChannelKind::AnnouncementThread),
            (11, ChannelKind::PublicThread),
            (12, ChannelKind::PrivateThread),
            (13, ChannelKind::GuildStageVoice),
            (14, ChannelKind::GuildDirectory),
            (15, ChannelKind::GuildForum),
            (16, ChannelKind::GuildMedia),
        ] {
            assert_eq!(ChannelKind::from_i64(value), kind);
            assert_eq!(kind.as_i64(), value);

            let json = serde_json::to_value(kind).expect("serialisable");
            assert_eq!(json, serde_json::json!(value));

            let parsed: ChannelKind = serde_json::from_value(json).expect("deserialisable");
            assert_eq!(parsed, kind);
        }
    }

    #[test]
    fn unknown_channel_kinds_are_preserved() {
        let parsed: ChannelKind = serde_json::from_value(serde_json::json!(42)).expect("parsed");

        assert_eq!(parsed, ChannelKind::Unknown(42));
        assert_eq!(parsed.as_i64(), 42);
        assert!(!parsed.is_thread());
        assert!(!ChannelKind::Unknown(10).is_thread());
    }

    #[test]
    fn threads_are_the_three_documented_kinds() {
        assert!(ChannelKind::AnnouncementThread.is_thread());
        assert!(ChannelKind::PublicThread.is_thread());
        assert!(ChannelKind::PrivateThread.is_thread());
        assert!(!ChannelKind::GuildForum.is_thread());
        assert!(!ChannelKind::GuildText.is_thread());
    }

    #[test]
    fn serialises_kind_as_the_integer_type_field() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": "1",
            "type": 15,
            "name": "forum"
        }))
        .expect("parsed");

        let json = serde_json::to_value(&channel).expect("serialisable");

        assert_eq!(json["type"], serde_json::json!(15));
        assert_eq!(json["id"], serde_json::json!("1"));
    }
}

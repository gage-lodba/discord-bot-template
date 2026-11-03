//! Guild and channel permissions.
//!
//! A permission set is a 64-bit bitfield. Discord serialises it as a decimal
//! string on the wire, so [`Permissions`] has custom `Serialize`/`Deserialize`
//! implementations.
//!
//! Permission resolution (the hierarchy a client applies to decide whether a
//! member may do something in a channel) is implemented on
//! [`Guild::permissions_for`](crate::model::guild::Guild::permissions_for) where
//! it has access to the guild's roles and the channel's overwrites.
//!
//! Reference: `docs/permissions.md`.

use std::fmt;
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A set of Discord permissions.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Permissions(u64);

impl Permissions {
    /// Builds a permission set from raw bits, dropping bits this crate does not
    /// know about.
    ///
    /// Unknown bits are dropped rather than preserved so that a future Discord
    /// permission can never accidentally be treated as one of the permissions
    /// this crate checks.
    #[must_use]
    pub const fn from_bits_truncate(bits: u64) -> Self {
        Self(bits & Self::KNOWN_BITS)
    }

    /// Returns the raw bits.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Returns an empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns true when no permission is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true when every permission in `required` is set.
    #[must_use]
    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    /// Returns true when at least one permission in `other` is set.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Returns the union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns the intersection of two sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Returns the permissions in `self` that are not in `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Adds permissions.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes permissions.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Returns true when the set contains [`Permissions::ADMINISTRATOR`].
    #[must_use]
    pub const fn is_administrator(self) -> bool {
        self.contains(Self::ADMINISTRATOR)
    }

    // --- Guild-level permissions (docs/permissions.md, "Bitwise Permission Flags") ---

    /// Allows creation of instant invites.
    pub const CREATE_INSTANT_INVITE: Self = Self(1 << 0);
    /// Allows kicking members.
    pub const KICK_MEMBERS: Self = Self(1 << 1);
    /// Allows banning members.
    pub const BAN_MEMBERS: Self = Self(1 << 2);
    /// Allows all permissions and bypasses channel overwrites.
    pub const ADMINISTRATOR: Self = Self(1 << 3);
    /// Allows management and editing of channels.
    pub const MANAGE_CHANNELS: Self = Self(1 << 4);
    /// Allows management and editing of the guild.
    pub const MANAGE_GUILD: Self = Self(1 << 5);
    /// Allows adding reactions to messages.
    pub const ADD_REACTIONS: Self = Self(1 << 6);
    /// Allows viewing the audit log.
    pub const VIEW_AUDIT_LOG: Self = Self(1 << 7);
    /// Allows using priority speaker in a voice channel.
    pub const PRIORITY_SPEAKER: Self = Self(1 << 8);
    /// Allows going live in a voice channel.
    pub const STREAM: Self = Self(1 << 9);
    /// Allows viewing a channel.
    pub const VIEW_CHANNEL: Self = Self(1 << 10);
    /// Allows sending messages in a channel.
    pub const SEND_MESSAGES: Self = Self(1 << 11);
    /// Allows sending `/tts` messages.
    pub const SEND_TTS_MESSAGES: Self = Self(1 << 12);
    /// Allows deleting other members' messages.
    pub const MANAGE_MESSAGES: Self = Self(1 << 13);
    /// Allows links to be auto-embedded.
    pub const EMBED_LINKS: Self = Self(1 << 14);
    /// Allows uploading images and files.
    pub const ATTACH_FILES: Self = Self(1 << 15);
    /// Allows reading message history.
    pub const READ_MESSAGE_HISTORY: Self = Self(1 << 16);
    /// Allows mentioning `@everyone` and `@here`.
    pub const MENTION_EVERYONE: Self = Self(1 << 17);
    /// Allows using custom emojis from other guilds.
    pub const USE_EXTERNAL_EMOJIS: Self = Self(1 << 18);
    /// Allows viewing guild insights.
    pub const VIEW_GUILD_INSIGHTS: Self = Self(1 << 19);
    /// Allows joining a voice channel.
    pub const CONNECT: Self = Self(1 << 20);
    /// Allows speaking in a voice channel.
    pub const SPEAK: Self = Self(1 << 21);
    /// Allows muting members in a voice channel.
    pub const MUTE_MEMBERS: Self = Self(1 << 22);
    /// Allows deafening members in a voice channel.
    pub const DEAFEN_MEMBERS: Self = Self(1 << 23);
    /// Allows moving members between voice channels.
    pub const MOVE_MEMBERS: Self = Self(1 << 24);
    /// Allows using voice-activity detection.
    pub const USE_VAD: Self = Self(1 << 25);
    /// Allows changing the member's own nickname.
    pub const CHANGE_NICKNAME: Self = Self(1 << 26);
    /// Allows changing other members' nicknames.
    pub const MANAGE_NICKNAMES: Self = Self(1 << 27);
    /// Allows managing and editing roles.
    pub const MANAGE_ROLES: Self = Self(1 << 28);
    /// Allows managing and editing webhooks.
    pub const MANAGE_WEBHOOKS: Self = Self(1 << 29);
    /// Allows editing and deleting emojis, stickers and soundboard sounds.
    pub const MANAGE_GUILD_EXPRESSIONS: Self = Self(1 << 30);
    /// Allows using application commands.
    pub const USE_APPLICATION_COMMANDS: Self = Self(1 << 31);
    /// Allows requesting to speak in stage channels.
    pub const REQUEST_TO_SPEAK: Self = Self(1 << 32);
    /// Allows editing and deleting scheduled events.
    pub const MANAGE_EVENTS: Self = Self(1 << 33);
    /// Allows deleting and archiving threads.
    pub const MANAGE_THREADS: Self = Self(1 << 34);
    /// Allows creating public and announcement threads.
    pub const CREATE_PUBLIC_THREADS: Self = Self(1 << 35);
    /// Allows creating private threads.
    pub const CREATE_PRIVATE_THREADS: Self = Self(1 << 36);
    /// Allows using custom stickers from other guilds.
    pub const USE_EXTERNAL_STICKERS: Self = Self(1 << 37);
    /// Allows sending messages in threads.
    pub const SEND_MESSAGES_IN_THREADS: Self = Self(1 << 38);
    /// Allows using Activities.
    pub const USE_EMBEDDED_ACTIVITIES: Self = Self(1 << 39);
    /// Allows timing out members.
    pub const MODERATE_MEMBERS: Self = Self(1 << 40);
    /// Allows viewing role subscription insights.
    pub const VIEW_CREATOR_MONETIZATION_ANALYTICS: Self = Self(1 << 41);
    /// Allows using soundboard in a voice channel.
    pub const USE_SOUNDBOARD: Self = Self(1 << 42);
    /// Allows creating emojis, stickers and soundboard sounds.
    pub const CREATE_GUILD_EXPRESSIONS: Self = Self(1 << 43);
    /// Allows creating scheduled events.
    pub const CREATE_EVENTS: Self = Self(1 << 44);
    /// Allows using custom soundboard sounds from other guilds.
    pub const USE_EXTERNAL_SOUNDS: Self = Self(1 << 45);
    /// Allows sending voice messages.
    pub const SEND_VOICE_MESSAGES: Self = Self(1 << 46);
    /// Allows setting voice channel status.
    pub const SET_VOICE_CHANNEL_STATUS: Self = Self(1 << 48);
    /// Allows sending polls.
    pub const SEND_POLLS: Self = Self(1 << 49);
    /// Allows user-installed apps to send public responses.
    pub const USE_EXTERNAL_APPS: Self = Self(1 << 50);
    /// Allows pinning and unpinning messages.
    pub const PIN_MESSAGES: Self = Self(1 << 51);
    /// Allows bypassing slowmode restrictions.
    pub const BYPASS_SLOWMODE: Self = Self(1 << 52);

    /// Every permission bit this crate knows about.
    pub const KNOWN_BITS: u64 = ((1 << 47) - 1) // bits 0..=46
        | (0b1_1111 << 48) // bits 48..=52
        ;
}

impl BitOr for Permissions {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for Permissions {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

impl BitAnd for Permissions {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        self.intersection(rhs)
    }
}

impl BitAndAssign for Permissions {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl fmt::Debug for Permissions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Permissions(0x{:x})", self.0)
    }
}

impl Serialize for Permissions {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // docs/permissions.md: "In API v8 and above, all permissions are
        // serialized as strings".
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PermissionsVisitor;

        impl Visitor<'_> for PermissionsVisitor {
            type Value = u64;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a permission bitfield as a decimal string or an integer")
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                Ok(value)
            }
        }

        Ok(Self::from_bits_truncate(
            deserializer.deserialize_any(PermissionsVisitor)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_checks_every_requested_bit() {
        let granted = Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS;

        assert!(granted.contains(Permissions::KICK_MEMBERS));
        assert!(granted.contains(Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS));
        assert!(!granted.contains(Permissions::MANAGE_ROLES));
        assert!(granted.intersects(Permissions::BAN_MEMBERS | Permissions::MANAGE_ROLES));
    }

    #[test]
    fn administrator_is_a_single_bit() {
        assert_eq!(Permissions::ADMINISTRATOR.bits(), 0b1000);
        assert!(Permissions::ADMINISTRATOR.is_administrator());
        assert!(!Permissions::KICK_MEMBERS.is_administrator());
    }

    #[test]
    fn deserializes_the_wire_format_as_a_string() {
        let parsed: Permissions = serde_json::from_str("\"2048\"").expect("decimal string");

        assert_eq!(parsed, Permissions::SEND_MESSAGES);
    }

    #[test]
    fn serializes_as_a_decimal_string() {
        let json = serde_json::to_string(&(Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS))
            .expect("serialisable");

        assert_eq!(json, "\"6\"");
    }

    #[test]
    fn unknown_bits_are_dropped() {
        // Bit 63 is not a permission this crate models.
        let parsed: Permissions = serde_json::from_str("\"9223372036854775808\"").expect("parsed");

        assert_eq!(parsed, Permissions::empty());
    }

    #[test]
    fn difference_removes_only_the_requested_bits() {
        let all = Permissions::KICK_MEMBERS | Permissions::BAN_MEMBERS;

        assert_eq!(
            all.difference(Permissions::KICK_MEMBERS),
            Permissions::BAN_MEMBERS
        );
    }

    #[test]
    fn known_bits_cover_the_documented_flags() {
        for permission in [
            Permissions::CREATE_INSTANT_INVITE,
            Permissions::BYPASS_SLOWMODE,
            Permissions::SEND_VOICE_MESSAGES,
            Permissions::SET_VOICE_CHANNEL_STATUS,
        ] {
            assert_eq!(
                Permissions::from_bits_truncate(permission.bits()),
                permission,
                "documented flag was dropped as unknown"
            );
        }
    }

    /// The permission flags `docs/permissions.md` (Bitwise Permission Flags)
    /// assigns: the documented name, the bit number the table prints, and the
    /// matching [`Permissions`] constant.
    ///
    /// The table assigns bits 0-46 and 48-52; bit 47 is not a documented
    /// permission.
    const DOCUMENTED_PERMISSIONS: &[(&str, u32, Permissions)] = &[
        (
            "CREATE_INSTANT_INVITE",
            0,
            Permissions::CREATE_INSTANT_INVITE,
        ),
        ("KICK_MEMBERS", 1, Permissions::KICK_MEMBERS),
        ("BAN_MEMBERS", 2, Permissions::BAN_MEMBERS),
        ("ADMINISTRATOR", 3, Permissions::ADMINISTRATOR),
        ("MANAGE_CHANNELS", 4, Permissions::MANAGE_CHANNELS),
        ("MANAGE_GUILD", 5, Permissions::MANAGE_GUILD),
        ("ADD_REACTIONS", 6, Permissions::ADD_REACTIONS),
        ("VIEW_AUDIT_LOG", 7, Permissions::VIEW_AUDIT_LOG),
        ("PRIORITY_SPEAKER", 8, Permissions::PRIORITY_SPEAKER),
        ("STREAM", 9, Permissions::STREAM),
        ("VIEW_CHANNEL", 10, Permissions::VIEW_CHANNEL),
        ("SEND_MESSAGES", 11, Permissions::SEND_MESSAGES),
        ("SEND_TTS_MESSAGES", 12, Permissions::SEND_TTS_MESSAGES),
        ("MANAGE_MESSAGES", 13, Permissions::MANAGE_MESSAGES),
        ("EMBED_LINKS", 14, Permissions::EMBED_LINKS),
        ("ATTACH_FILES", 15, Permissions::ATTACH_FILES),
        (
            "READ_MESSAGE_HISTORY",
            16,
            Permissions::READ_MESSAGE_HISTORY,
        ),
        ("MENTION_EVERYONE", 17, Permissions::MENTION_EVERYONE),
        ("USE_EXTERNAL_EMOJIS", 18, Permissions::USE_EXTERNAL_EMOJIS),
        ("VIEW_GUILD_INSIGHTS", 19, Permissions::VIEW_GUILD_INSIGHTS),
        ("CONNECT", 20, Permissions::CONNECT),
        ("SPEAK", 21, Permissions::SPEAK),
        ("MUTE_MEMBERS", 22, Permissions::MUTE_MEMBERS),
        ("DEAFEN_MEMBERS", 23, Permissions::DEAFEN_MEMBERS),
        ("MOVE_MEMBERS", 24, Permissions::MOVE_MEMBERS),
        ("USE_VAD", 25, Permissions::USE_VAD),
        ("CHANGE_NICKNAME", 26, Permissions::CHANGE_NICKNAME),
        ("MANAGE_NICKNAMES", 27, Permissions::MANAGE_NICKNAMES),
        ("MANAGE_ROLES", 28, Permissions::MANAGE_ROLES),
        ("MANAGE_WEBHOOKS", 29, Permissions::MANAGE_WEBHOOKS),
        (
            "MANAGE_GUILD_EXPRESSIONS",
            30,
            Permissions::MANAGE_GUILD_EXPRESSIONS,
        ),
        (
            "USE_APPLICATION_COMMANDS",
            31,
            Permissions::USE_APPLICATION_COMMANDS,
        ),
        ("REQUEST_TO_SPEAK", 32, Permissions::REQUEST_TO_SPEAK),
        ("MANAGE_EVENTS", 33, Permissions::MANAGE_EVENTS),
        ("MANAGE_THREADS", 34, Permissions::MANAGE_THREADS),
        (
            "CREATE_PUBLIC_THREADS",
            35,
            Permissions::CREATE_PUBLIC_THREADS,
        ),
        (
            "CREATE_PRIVATE_THREADS",
            36,
            Permissions::CREATE_PRIVATE_THREADS,
        ),
        (
            "USE_EXTERNAL_STICKERS",
            37,
            Permissions::USE_EXTERNAL_STICKERS,
        ),
        (
            "SEND_MESSAGES_IN_THREADS",
            38,
            Permissions::SEND_MESSAGES_IN_THREADS,
        ),
        (
            "USE_EMBEDDED_ACTIVITIES",
            39,
            Permissions::USE_EMBEDDED_ACTIVITIES,
        ),
        ("MODERATE_MEMBERS", 40, Permissions::MODERATE_MEMBERS),
        (
            "VIEW_CREATOR_MONETIZATION_ANALYTICS",
            41,
            Permissions::VIEW_CREATOR_MONETIZATION_ANALYTICS,
        ),
        ("USE_SOUNDBOARD", 42, Permissions::USE_SOUNDBOARD),
        (
            "CREATE_GUILD_EXPRESSIONS",
            43,
            Permissions::CREATE_GUILD_EXPRESSIONS,
        ),
        ("CREATE_EVENTS", 44, Permissions::CREATE_EVENTS),
        ("USE_EXTERNAL_SOUNDS", 45, Permissions::USE_EXTERNAL_SOUNDS),
        ("SEND_VOICE_MESSAGES", 46, Permissions::SEND_VOICE_MESSAGES),
        (
            "SET_VOICE_CHANNEL_STATUS",
            48,
            Permissions::SET_VOICE_CHANNEL_STATUS,
        ),
        ("SEND_POLLS", 49, Permissions::SEND_POLLS),
        ("USE_EXTERNAL_APPS", 50, Permissions::USE_EXTERNAL_APPS),
        ("PIN_MESSAGES", 51, Permissions::PIN_MESSAGES),
        ("BYPASS_SLOWMODE", 52, Permissions::BYPASS_SLOWMODE),
    ];

    #[test]
    fn every_documented_permission_flag_has_the_documented_value() {
        let mut union = Permissions::empty();

        for &(name, position, permission) in DOCUMENTED_PERMISSIONS {
            let bit = 1u64 << position;
            assert_eq!(
                permission.bits(),
                bit,
                "{name} is not the documented 1 << {position}"
            );
            union = union.union(permission);
        }

        // `KNOWN_BITS` must be exactly the set of documented permissions: no
        // documented bit dropped and no undocumented bit invented.
        assert_eq!(union.bits(), Permissions::KNOWN_BITS);
        assert_eq!(
            union.bits().count_ones(),
            u32::try_from(DOCUMENTED_PERMISSIONS.len()).expect("the table fits in a u32"),
            "two documented permissions share a bit"
        );
    }
}

//! The user object and the CDN URLs derived from it.
//!
//! Reference: `docs/user.md` (User Object) and `docs/reference.md`
//! (Image Formatting: Default User Avatar).

use serde::{Deserialize, Serialize};

use crate::id::UserId;

/// The base Discord entity: a user account.
///
/// Only the fields this crate consumes are modelled; everything else Discord
/// sends is ignored by serde. Optional fields that the reference marks with `?`
/// are [`Option`]s, and `bot`/`system` have defaults because they are only
/// present on user objects Discord returns for a full account.
///
/// Reference: `docs/user.md` (User Structure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// The user's snowflake id.
    pub id: UserId,
    /// The user's username, not unique across the platform.
    pub username: String,
    /// The user's Discord-tag. Migrated accounts use `"0"`.
    #[serde(default)]
    pub discriminator: String,
    /// The user's display name, if it is set.
    #[serde(default)]
    pub global_name: Option<String>,
    /// The user's avatar hash, not a URL. `None` means the default avatar.
    #[serde(default)]
    pub avatar: Option<String>,
    /// Whether the user belongs to an `OAuth2` application (a bot account).
    #[serde(default)]
    pub bot: bool,
    /// Whether the user is an Official Discord System user.
    #[serde(default)]
    pub system: bool,
    /// The public flags on the user's account (`docs/user.md`, User Flags).
    #[serde(default)]
    pub flags: Option<u64>,
    /// The user's banner colour, encoded as an integer representation of a
    /// hexadecimal colour code.
    #[serde(default)]
    pub accent_color: Option<u32>,
    /// The user's banner hash, if they have one.
    #[serde(default)]
    pub banner: Option<String>,
}

impl User {
    /// Returns [`User::global_name`] when set, otherwise the username.
    ///
    /// Reference: `docs/user.md` (Usernames and Nicknames).
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    /// Returns the CDN URL of the user's avatar.
    ///
    /// Falls back to [`User::default_avatar_url`] when the user has no avatar
    /// hash set.
    ///
    /// Reference: `docs/reference.md` (Image Formatting, CDN Endpoints).
    #[must_use]
    pub fn avatar_url(&self) -> String {
        match &self.avatar {
            Some(hash) => format!(
                "https://cdn.discordapp.com/avatars/{}/{hash}.png",
                self.id.get()
            ),
            None => self.default_avatar_url(),
        }
    }

    /// Returns the CDN URL of the user's default avatar.
    ///
    /// The index documented in `docs/reference.md` (Image Formatting, footnote
    /// `**`) depends on the username system:
    ///
    /// * accounts migrated to the new username system (discriminator `"0"`, or
    ///   no discriminator at all) use `(user_id >> 22) % 6`;
    /// * legacy accounts use `discriminator % 5`.
    #[must_use]
    pub fn default_avatar_url(&self) -> String {
        format!(
            "https://cdn.discordapp.com/embed/avatars/{}.png",
            self.default_avatar_index()
        )
    }

    /// Returns the default-avatar index for this user.
    fn default_avatar_index(&self) -> u64 {
        match self.discriminator.parse::<u64>() {
            // "0" (and a missing discriminator) means the account uses the new
            // username system, where the default avatar no longer depends on
            // the discriminator.
            Ok(0) | Err(_) => (self.id.get() >> 22) % 6,
            Ok(discriminator) => discriminator % 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_example_user_ignoring_unknown_fields() {
        let user: User = serde_json::from_value(serde_json::json!({
            "id": "80351110224678912",
            "username": "Nelly",
            "global_name": null,
            "discriminator": "1337",
            "avatar": "8342729096ea3675442027381ff50dfe",
            "verified": true,
            "email": "nelly@discord.com",
            "flags": 64,
            "banner": "06c16474723fe537c283b8efa61a30c8",
            "accent_color": 16_711_680,
            "premium_type": 0,
            "public_flags": 64
        }))
        .expect("documented user payload");

        assert_eq!(user.id, UserId::new(80_351_110_224_678_912));
        assert_eq!(user.username, "Nelly");
        assert_eq!(user.discriminator, "1337");
        assert_eq!(user.global_name, None);
        assert_eq!(
            user.avatar.as_deref(),
            Some("8342729096ea3675442027381ff50dfe")
        );
        assert!(!user.bot);
        assert!(!user.system);
        assert_eq!(user.flags, Some(64));
        assert_eq!(user.accent_color, Some(16_711_680));
        assert_eq!(
            user.banner.as_deref(),
            Some("06c16474723fe537c283b8efa61a30c8")
        );
    }

    #[test]
    fn missing_optional_fields_use_defaults() {
        let user: User = serde_json::from_value(serde_json::json!({
            "id": "2",
            "username": "minimal"
        }))
        .expect("minimal user payload");

        assert_eq!(user.discriminator, "");
        assert_eq!(user.global_name, None);
        assert_eq!(user.avatar, None);
        assert!(!user.bot);
        assert!(!user.system);
        assert_eq!(user.flags, None);
        assert_eq!(user.accent_color, None);
        assert_eq!(user.banner, None);
    }

    #[test]
    fn display_name_prefers_global_name() {
        let mut user = User {
            id: UserId::new(1),
            username: "old_name".to_owned(),
            discriminator: "0".to_owned(),
            global_name: None,
            avatar: None,
            bot: false,
            system: false,
            flags: None,
            accent_color: None,
            banner: None,
        };

        assert_eq!(user.display_name(), "old_name");

        user.global_name = Some("New Name".to_owned());
        assert_eq!(user.display_name(), "New Name");
    }

    #[test]
    fn avatar_url_uses_the_avatar_hash_when_present() {
        let user = User {
            id: UserId::new(80_351_110_224_678_912),
            username: "Nelly".to_owned(),
            discriminator: "1337".to_owned(),
            global_name: None,
            avatar: Some("8342729096ea3675442027381ff50dfe".to_owned()),
            bot: false,
            system: false,
            flags: None,
            accent_color: None,
            banner: None,
        };

        assert_eq!(
            user.avatar_url(),
            "https://cdn.discordapp.com/avatars/80351110224678912/8342729096ea3675442027381ff50dfe.png"
        );
    }

    #[test]
    fn legacy_default_avatar_uses_discriminator_mod_five() {
        let user = User {
            id: UserId::new(80_351_110_224_678_912),
            username: "Nelly".to_owned(),
            discriminator: "1337".to_owned(),
            global_name: None,
            avatar: None,
            bot: false,
            system: false,
            flags: None,
            accent_color: None,
            banner: None,
        };

        // 1337 % 5 = 2
        assert_eq!(
            user.default_avatar_url(),
            "https://cdn.discordapp.com/embed/avatars/2.png"
        );
        assert_eq!(user.avatar_url(), user.default_avatar_url());
    }

    #[test]
    fn migrated_default_avatar_uses_the_snowflake() {
        // (25165824 >> 22) = 6; 6 % 6 = 0.
        let migrated = User {
            id: UserId::new(25_165_824),
            username: "newname".to_owned(),
            discriminator: "0".to_owned(),
            global_name: None,
            avatar: None,
            bot: false,
            system: false,
            flags: None,
            accent_color: None,
            banner: None,
        };

        assert_eq!(
            migrated.default_avatar_url(),
            "https://cdn.discordapp.com/embed/avatars/0.png"
        );

        // No discriminator at all is treated the same way.
        let mut absent = migrated.clone();
        absent.discriminator = String::new();
        assert_eq!(absent.default_avatar_url(), migrated.default_avatar_url());
    }

    #[test]
    fn serialises_and_deserialises_without_losing_fields() {
        let user: User = serde_json::from_value(serde_json::json!({
            "id": "42",
            "username": "bot",
            "discriminator": "0",
            "global_name": "Bot",
            "avatar": null,
            "bot": true,
            "system": false,
            "flags": 64,
            "accent_color": 123_456,
            "banner": null
        }))
        .expect("user payload");

        let json = serde_json::to_value(&user).expect("serialisable");
        let parsed: User = serde_json::from_value(json).expect("round trip");

        assert_eq!(parsed, user);
        assert!(user.bot);
    }
}

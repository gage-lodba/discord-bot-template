//! Emoji models and the reaction/component emoji shape.
//!
//! Reference: `docs/emoji.md` (Emoji Object, Custom Emoji Examples) and
//! `docs/components-reference.md` (Button Structure and Select Option
//! Structure: the partial emoji object `name`, `id` and `animated` that message
//! components accept).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::id::{EmojiId, RoleId};
use crate::model::user::User;

/// A guild or application emoji.
///
/// `id` is `None` for standard unicode emoji and `Some` for custom emoji;
/// `name` may be `None` for a custom emoji whose data is not available (for
/// example one deleted from its guild).
///
/// The three capability booleans of the documented object are grouped in
/// [`EmojiFlags`] and flattened back onto the emoji object, so the serialised
/// JSON keeps the documented set of top-level keys.
///
/// Reference: `docs/emoji.md` (Emoji Structure).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emoji {
    /// The emoji id, or `None` for a standard unicode emoji.
    #[serde(default)]
    pub id: Option<EmojiId>,
    /// The emoji name; for unicode emoji this is the character itself.
    #[serde(default)]
    pub name: Option<String>,
    /// Whether the emoji is animated.
    #[serde(default)]
    pub animated: bool,
    /// The role ids allowed to use this emoji.
    #[serde(default)]
    pub roles: Vec<RoleId>,
    /// The user that created this emoji.
    #[serde(default)]
    pub user: Option<User>,
    /// The emoji's capability flags, flattened onto the emoji object.
    #[serde(flatten)]
    pub flags: EmojiFlags,
}

/// The capability flags of an [`Emoji`].
///
/// The documented Emoji Object lists `require_colons`, `managed` and `available`
/// as three top-level booleans. Grouping them keeps [`Emoji`] from being a bag
/// of booleans while `#[serde(flatten)]` preserves the documented JSON shape.
///
/// Reference: `docs/emoji.md` (Emoji Structure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmojiFlags {
    /// Whether this emoji must be wrapped in colons.
    #[serde(default)]
    pub require_colons: bool,
    /// Whether this emoji is managed.
    #[serde(default)]
    pub managed: bool,
    /// Whether this emoji can be used; may be false due to loss of boosts.
    #[serde(default)]
    pub available: bool,
}

/// An emoji as accepted by message components and reactions.
///
/// The component shape is a partial emoji object carrying `name`, `id` and
/// `animated` (`docs/components-reference.md`, Button Structure and Select
/// Option Structure); the reaction shape is the custom emoji example in
/// `docs/emoji.md` (Custom Emoji Examples). See also `CONTRACT.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionType {
    /// A standard unicode emoji, serialised as `{"name": "🔥"}`.
    Unicode(String),
    /// A custom emoji, serialised as
    /// `{"id": "...", "name": "...", "animated": bool}`; `name` and `animated`
    /// are only emitted when set.
    Custom {
        /// The emoji id.
        id: EmojiId,
        /// The emoji name, when Discord included it.
        name: Option<String>,
        /// Whether the emoji is animated.
        animated: bool,
    },
}

impl From<Emoji> for ReactionType {
    /// Converts an [`Emoji`] to the reaction shape: custom when the emoji has
    /// an id, unicode otherwise.
    fn from(emoji: Emoji) -> Self {
        match emoji.id {
            Some(id) => Self::Custom {
                id,
                name: emoji.name,
                animated: emoji.animated,
            },
            None => Self::Unicode(emoji.name.unwrap_or_default()),
        }
    }
}

impl From<&str> for ReactionType {
    fn from(name: &str) -> Self {
        Self::Unicode(name.to_owned())
    }
}

impl Serialize for ReactionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        match self {
            Self::Unicode(name) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("name", name)?;
                map.end()
            }
            Self::Custom { id, name, animated } => {
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("id", id)?;
                if let Some(name) = name {
                    map.serialize_entry("name", name)?;
                }
                if *animated {
                    map.serialize_entry("animated", &true)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for ReactionType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// The component/reaction emoji shape: `{id?, name?, animated?}`.
        #[derive(Deserialize)]
        struct WireEmoji {
            #[serde(default)]
            id: Option<EmojiId>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            animated: bool,
        }

        let wire = WireEmoji::deserialize(deserializer)?;

        match wire.id {
            Some(id) => Ok(Self::Custom {
                id,
                name: wire.name,
                animated: wire.animated,
            }),
            None => wire
                .name
                .map(Self::Unicode)
                .ok_or_else(|| serde::de::Error::missing_field("name")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_custom_emoji() {
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": "41771983429993937",
            "name": "LUL",
            "roles": ["41771983429993000", "41771983429993111"],
            "user": {
                "username": "Luigi",
                "discriminator": "0002",
                "id": "96008815106887111",
                "avatar": "5500909a3274e1812beb4e8de6631111",
                "public_flags": 131_328
            },
            "require_colons": true,
            "managed": false,
            "animated": false
        }))
        .expect("documented emoji payload");

        assert_eq!(emoji.id, Some(EmojiId::new(41_771_983_429_993_937)));
        assert_eq!(emoji.name.as_deref(), Some("LUL"));
        assert!(!emoji.animated);
        assert_eq!(
            emoji.roles,
            vec![
                RoleId::new(41_771_983_429_993_000),
                RoleId::new(41_771_983_429_993_111)
            ]
        );
        assert_eq!(
            emoji.user.as_ref().map(|user| user.username.as_str()),
            Some("Luigi")
        );
        assert!(emoji.flags.require_colons);
        assert!(!emoji.flags.managed);
        // The `available` field was not sent; it defaults to false.
        assert!(!emoji.flags.available);
    }

    #[test]
    fn serialises_the_documented_emoji_shape_with_flattened_flags() {
        // `docs/emoji.md` (Emoji Structure): `require_colons`, `managed` and
        // `available` stay top-level keys, not a nested object.
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": "41771983429993937",
            "name": "LUL",
            "roles": [],
            "require_colons": true,
            "managed": false,
            "animated": false,
            "available": true
        }))
        .expect("documented emoji payload");

        assert_eq!(
            serde_json::to_value(&emoji).expect("serialisable"),
            serde_json::json!({
                "id": "41771983429993937",
                "name": "LUL",
                "animated": false,
                "roles": [],
                "user": null,
                "require_colons": true,
                "managed": false,
                "available": true
            })
        );
    }

    #[test]
    fn parses_the_documented_standard_emoji() {
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": null,
            "name": "🔥"
        }))
        .expect("standard emoji payload");

        assert_eq!(emoji.id, None);
        assert_eq!(emoji.name.as_deref(), Some("🔥"));
        assert!(emoji.roles.is_empty());
        assert_eq!(emoji.user, None);
        // No capability flag was sent, so the flattened struct defaults them.
        assert!(!emoji.flags.require_colons);
        assert!(!emoji.flags.managed);
        assert!(!emoji.flags.available);
    }

    #[test]
    fn parses_a_custom_emoji_with_a_null_name() {
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": "41771983429993937",
            "name": null
        }))
        .expect("emoji with unavailable name");

        assert_eq!(emoji.id, Some(EmojiId::new(41_771_983_429_993_937)));
        assert_eq!(emoji.name, None);
    }

    #[test]
    fn converts_a_custom_emoji_to_a_reaction_type() {
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": "41771983429993937",
            "name": "LUL",
            "animated": true
        }))
        .expect("emoji payload");

        assert_eq!(
            ReactionType::from(emoji),
            ReactionType::Custom {
                id: EmojiId::new(41_771_983_429_993_937),
                name: Some("LUL".to_owned()),
                animated: true
            }
        );
    }

    #[test]
    fn converts_a_unicode_emoji_to_a_reaction_type() {
        let emoji: Emoji = serde_json::from_value(serde_json::json!({
            "id": null,
            "name": "🔥"
        }))
        .expect("emoji payload");

        assert_eq!(
            ReactionType::from(emoji),
            ReactionType::Unicode("🔥".to_owned())
        );
        assert_eq!(
            ReactionType::from("🔥"),
            ReactionType::Unicode("🔥".to_owned())
        );
    }

    #[test]
    fn serialises_a_unicode_reaction_as_the_component_emoji_shape() {
        let json =
            serde_json::to_value(ReactionType::Unicode("🔥".to_owned())).expect("serialisable");

        assert_eq!(json, serde_json::json!({"name": "🔥"}));
    }

    #[test]
    fn serialises_a_custom_reaction_as_the_component_emoji_shape() {
        let reaction = ReactionType::Custom {
            id: EmojiId::new(41_771_983_429_993_937),
            name: Some("LUL".to_owned()),
            animated: true,
        };

        let json = serde_json::to_value(&reaction).expect("serialisable");

        assert_eq!(
            json,
            serde_json::json!({
                "id": "41771983429993937",
                "name": "LUL",
                "animated": true
            })
        );

        // `animated` is optional in the reference, so it is omitted when false.
        let not_animated = ReactionType::Custom {
            id: EmojiId::new(41_771_983_429_993_937),
            name: None,
            animated: false,
        };
        let json = serde_json::to_value(&not_animated).expect("serialisable");
        assert_eq!(json, serde_json::json!({"id": "41771983429993937"}));
    }

    #[test]
    fn parses_both_reaction_wire_shapes() {
        let unicode: ReactionType = serde_json::from_value(serde_json::json!({
            "id": null,
            "name": "🔥"
        }))
        .expect("unicode reaction payload");
        assert_eq!(unicode, ReactionType::Unicode("🔥".to_owned()));

        let custom: ReactionType = serde_json::from_value(serde_json::json!({
            "id": "41771983429993937",
            "name": "LUL"
        }))
        .expect("custom reaction payload");
        assert_eq!(
            custom,
            ReactionType::Custom {
                id: EmojiId::new(41_771_983_429_993_937),
                name: Some("LUL".to_owned()),
                animated: false
            }
        );

        assert!(serde_json::from_value::<ReactionType>(serde_json::json!({})).is_err());
    }
}

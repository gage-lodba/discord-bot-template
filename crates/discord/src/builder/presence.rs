//! Presence and activity builders for the gateway's Update Presence (op 3).
//!
//! A presence update is an object with `since`, `activities`, `status` and
//! `afk`; each activity is an Activity Object with a `name`, a numeric `type`
//! and, for streaming activities, a `url`.
//!
//! Reference: `docs/gateway-events.md` (Update Presence, Status Types, Activity
//! Object).

use serde_json::{Map, Value};

/// A user's presence status.
///
/// The wire values are listed in `docs/gateway-events.md` (Status Types).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Status {
    /// Shown as online (`"online"`).
    #[default]
    Online,
    /// Shown as away (`"idle"`).
    Idle,
    /// Shown as do not disturb (`"dnd"`).
    DoNotDisturb,
    /// Shown as offline, but the user is online (`"invisible"`).
    Invisible,
    /// Shown as offline (`"offline"`).
    Offline,
}

impl Status {
    /// Returns the string Discord expects in the `status` field.
    ///
    /// Reference: `docs/gateway-events.md` (Status Types).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Idle => "idle",
            Self::DoNotDisturb => "dnd",
            Self::Invisible => "invisible",
            Self::Offline => "offline",
        }
    }
}

/// An activity shown on the bot's presence.
///
/// Bot users may only set `name`, `state`, `type` and `url`
/// (`docs/gateway-events.md`, Activity Object, note under the structure
/// table), which is exactly the subset this enum models.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Activity {
    /// A custom status. Wire type `4`.
    ///
    /// `docs/gateway-events.md` (Activity Types) formats a custom status as
    /// `{emoji} {state}`, and the same page notes bots may set `name`, `state`,
    /// `type` and `url`. The text is therefore sent as both `name` (the activity
    /// name Discord requires) and `state` (the field the client renders for a
    /// custom status), so the status actually shows its text.
    ///
    /// Reference: `docs/gateway-events.md` (Activity Object, Activity Types).
    Custom {
        /// The text shown in the status.
        name: String,
    },
    /// "Playing {name}". Wire type `0`.
    Playing {
        /// The activity name.
        name: String,
    },
    /// "Listening to {name}". Wire type `2`.
    Listening {
        /// The activity name.
        name: String,
    },
    /// "Watching {name}". Wire type `3`.
    Watching {
        /// The activity name.
        name: String,
    },
    /// "Competing in {name}". Wire type `5`.
    Competing {
        /// The activity name.
        name: String,
    },
    /// "Streaming {name}". Wire type `1`.
    ///
    /// Reference: `docs/gateway-events.md` (Activity Types): the `url` is
    /// validated when the type is streaming.
    Streaming {
        /// The activity name.
        name: String,
        /// The stream URL; only Twitch and `YouTube` URLs are accepted.
        url: String,
    },
}

impl Activity {
    /// Creates a custom status (`"type": 4`).
    ///
    /// The text is serialised as both `name` and `state`, per
    /// `docs/gateway-events.md` (Activity Types: custom is `{emoji} {state}` and
    /// bots may set `state`).
    ///
    /// ```
    /// use discord::builder::presence::Activity;
    ///
    /// assert_eq!(
    ///     Activity::custom("Playing with code.").to_json(),
    ///     serde_json::json!({
    ///         "name": "Playing with code.",
    ///         "state": "Playing with code.",
    ///         "type": 4
    ///     })
    /// );
    /// ```
    #[must_use]
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom { name: name.into() }
    }

    /// Returns the numeric activity type.
    ///
    /// Reference: `docs/gateway-events.md` (Activity Types).
    #[must_use]
    pub const fn kind(&self) -> u8 {
        match self {
            Self::Playing { .. } => 0,
            Self::Streaming { .. } => 1,
            Self::Listening { .. } => 2,
            Self::Watching { .. } => 3,
            Self::Custom { .. } => 4,
            Self::Competing { .. } => 5,
        }
    }

    /// Serialises this activity to the Activity Object wire shape.
    ///
    /// Reference: `docs/gateway-events.md` (Activity Object).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let name = match self {
            Self::Custom { name }
            | Self::Playing { name }
            | Self::Listening { name }
            | Self::Watching { name }
            | Self::Competing { name }
            | Self::Streaming { name, .. } => name,
        };

        let mut object = Map::new();
        object.insert("name".to_owned(), Value::String(name.clone()));
        object.insert("type".to_owned(), Value::from(self.kind()));

        if let Self::Streaming { url, .. } = self {
            object.insert("url".to_owned(), Value::String(url.clone()));
        }

        // Custom status text lives in `state` (the field the client renders for
        // type 4); `name` alone would leave the status without its text.
        // Reference: docs/gateway-events.md (Activity Object, Activity Types).
        if let Self::Custom { name } = self {
            object.insert("state".to_owned(), Value::String(name.clone()));
        }

        Value::Object(object)
    }
}

/// Builds a gateway Update Presence payload.
///
/// The presence object is documented in `docs/gateway-events.md` (Update
/// Presence). `since` is always `null` because the bot never marks itself AFK
/// with an idle timestamp.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresenceBuilder {
    status: Status,
    activities: Vec<Activity>,
    afk: bool,
}

impl PresenceBuilder {
    /// Creates an online presence with no activities.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status: Status::Online,
            activities: Vec::new(),
            afk: false,
        }
    }

    /// Sets the status.
    #[must_use]
    pub const fn status(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    /// Adds one activity.
    #[must_use]
    pub fn activity(mut self, activity: Activity) -> Self {
        self.activities.push(activity);
        self
    }

    /// Replaces the activities with the given list.
    #[must_use]
    pub fn activities(mut self, activities: impl IntoIterator<Item = Activity>) -> Self {
        self.activities = activities.into_iter().collect();
        self
    }

    /// Sets whether the user is marked as AFK.
    #[must_use]
    pub const fn afk(mut self, afk: bool) -> Self {
        self.afk = afk;
        self
    }

    /// Serialises the presence object.
    ///
    /// Shape: `{"since": null, "activities": [...], "status": "online",
    /// "afk": false}`; see `docs/gateway-events.md` (Update Presence).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let activities = self
            .activities
            .iter()
            .map(Activity::to_json)
            .collect::<Vec<_>>();

        let mut object = Map::new();
        object.insert("since".to_owned(), Value::Null);
        object.insert("activities".to_owned(), Value::Array(activities));
        object.insert(
            "status".to_owned(),
            Value::String(self.status.as_str().to_owned()),
        );
        object.insert("afk".to_owned(), Value::Bool(self.afk));

        Value::Object(object)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Activity, PresenceBuilder, Status};

    #[test]
    fn online_with_a_custom_activity_matches_the_documented_shape() {
        let presence = PresenceBuilder::new()
            .status(Status::Online)
            .activity(Activity::custom("Playing with code."));

        assert_eq!(
            presence.to_json(),
            json!({
                "since": null,
                "activities": [{
                    "name": "Playing with code.",
                    "state": "Playing with code.",
                    "type": 4
                }],
                "status": "online",
                "afk": false
            })
        );
    }

    #[test]
    fn status_wire_values_are_the_documented_strings() {
        assert_eq!(Status::Online.as_str(), "online");
        assert_eq!(Status::Idle.as_str(), "idle");
        assert_eq!(Status::DoNotDisturb.as_str(), "dnd");
        assert_eq!(Status::Invisible.as_str(), "invisible");
        assert_eq!(Status::Offline.as_str(), "offline");
    }

    #[test]
    fn every_activity_type_has_the_documented_number_and_streaming_carries_its_url() {
        assert_eq!(
            Activity::Playing {
                name: "Rocket League".to_owned()
            }
            .to_json(),
            json!({"name": "Rocket League", "type": 0})
        );
        assert_eq!(
            Activity::Streaming {
                name: "Rocket League".to_owned(),
                url: "https://www.twitch.tv/discord".to_owned(),
            }
            .to_json(),
            json!({"name": "Rocket League", "type": 1, "url": "https://www.twitch.tv/discord"})
        );
        assert_eq!(
            Activity::Listening {
                name: "Spotify".to_owned()
            }
            .to_json(),
            json!({"name": "Spotify", "type": 2})
        );
        assert_eq!(
            Activity::Watching {
                name: "YouTube Together".to_owned()
            }
            .to_json(),
            json!({"name": "YouTube Together", "type": 3})
        );
        assert_eq!(
            Activity::Competing {
                name: "Arena World Champions".to_owned()
            }
            .to_json(),
            json!({"name": "Arena World Champions", "type": 5})
        );
    }

    #[test]
    fn activity_kind_is_exposed() {
        assert_eq!(Activity::custom("x").kind(), 4);
        assert_eq!(
            Activity::Streaming {
                name: "x".to_owned(),
                url: "https://twitch.tv/x".to_owned()
            }
            .kind(),
            1
        );
    }

    #[test]
    fn activities_replace_and_afk_is_serialised() {
        let presence = PresenceBuilder::new()
            .status(Status::DoNotDisturb)
            .activity(Activity::custom("first"))
            .activities([Activity::Playing {
                name: "second".to_owned(),
            }])
            .afk(true);

        assert_eq!(
            presence.to_json(),
            json!({
                "since": null,
                "activities": [{"name": "second", "type": 0}],
                "status": "dnd",
                "afk": true
            })
        );
    }

    #[test]
    fn default_presence_is_online_with_no_activities() {
        assert_eq!(
            PresenceBuilder::default().to_json(),
            json!({"since": null, "activities": [], "status": "online", "afk": false})
        );
    }
}

//! Application command builders for the guild command registration endpoint.
//!
//! A command has a `name`, a `description`, an optional
//! `default_member_permissions` decimal permission string, and up to 25
//! options; each option has an integer `type`, a name, a description and
//! optional `required`/`min_value`/`max_value`/`channel_types`/`choices`
//! fields. A choice value is a string or an integer (`docs/application-commands.md`,
//! Application Command Option Choice Structure).
//!
//! The wire shape is documented in `docs/application-commands.md`
//! (Application Command Structure, Application Command Option Structure); the
//! `default_member_permissions` field is a decimal permission string, matching
//! the serialisation documented in `docs/permissions.md`.
//!
//! [`CommandBuilder::to_json`] produces the JSON object sent by the HTTP layer:
//!
//! ```json
//! {"name":"ban","description":"Bans a user from the guild.",
//!  "default_member_permissions":"4","options":[
//!    {"type":6,"name":"target","description":"...","required":true},
//!    {"type":4,"name":"duration","description":"...","min_value":0,"max_value":7}]}
//! ```

use serde_json::{Map, Value};

use crate::model::application::CommandOptionKind;
use crate::model::channel::ChannelKind;
use crate::permissions::Permissions;

use super::limits;

/// The value of one application command option choice.
///
/// Reference: `docs/application-commands.md` (Application Command Option Choice
/// Structure) — a choice value is a string or an integer, and its type follows
/// the option's type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOptionChoiceValue {
    /// A string choice value (for `STRING` options, at most 100 characters).
    String(String),
    /// An integer choice value (for `INTEGER` options).
    Integer(i64),
}

impl From<&str> for CommandOptionChoiceValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for CommandOptionChoiceValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i64> for CommandOptionChoiceValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

/// Builds an application command definition for registration.
#[derive(Clone, Debug)]
pub struct CommandBuilder {
    name: String,
    description: String,
    options: Vec<CommandOptionBuilder>,
    default_member_permissions: Option<Permissions>,
    dm_permission: Option<bool>,
    nsfw: Option<bool>,
}

impl CommandBuilder {
    /// Creates a command with the given name and description.
    ///
    /// Application-command names are lowercase with no spaces according to the
    /// application-commands reference; this builder does not rewrite them.
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            options: Vec::new(),
            default_member_permissions: None,
            dm_permission: None,
            nsfw: None,
        }
    }

    /// Appends an option.
    ///
    /// Options beyond the documented maximum of 25 are ignored by
    /// [`Self::to_json`].
    #[must_use]
    pub fn add_option(mut self, option: CommandOptionBuilder) -> Self {
        self.options.push(option);
        self
    }

    /// Sets the permissions members need by default to use the command.
    ///
    /// Serialised as a decimal permission string, per `docs/permissions.md`
    /// (Permissions) and `docs/application-commands.md` (Application Command Structure).
    #[must_use]
    pub const fn default_member_permissions(mut self, permissions: Permissions) -> Self {
        self.default_member_permissions = Some(permissions);
        self
    }

    /// Whether the command is usable in direct messages.
    #[must_use]
    pub const fn dm_permission(mut self, enabled: bool) -> Self {
        self.dm_permission = Some(enabled);
        self
    }

    /// Whether the command is marked as age-restricted.
    #[must_use]
    pub const fn nsfw(mut self, nsfw: bool) -> Self {
        self.nsfw = Some(nsfw);
        self
    }

    /// Serialises the command definition.
    ///
    /// Shape documented in `docs/application-commands.md` (Application Command Structure). The caller
    /// registers it with `PUT /applications/{application_id}/guilds/{guild_id}/commands`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("name".to_owned(), Value::String(self.name.clone()));
        object.insert(
            "description".to_owned(),
            Value::String(self.description.clone()),
        );

        if let Some(permissions) = self.default_member_permissions {
            object.insert(
                "default_member_permissions".to_owned(),
                Value::String(permissions.bits().to_string()),
            );
        }

        if let Some(dm_permission) = self.dm_permission {
            object.insert("dm_permission".to_owned(), Value::Bool(dm_permission));
        }

        if let Some(nsfw) = self.nsfw {
            object.insert("nsfw".to_owned(), Value::Bool(nsfw));
        }

        if !self.options.is_empty() {
            let options = self
                .options
                .iter()
                .take(limits::COMMAND_OPTIONS)
                .map(CommandOptionBuilder::to_json)
                .collect();

            object.insert("options".to_owned(), Value::Array(options));
        }

        Value::Object(object)
    }
}

/// Builds one application command option.
#[derive(Clone, Debug)]
pub struct CommandOptionBuilder {
    kind: CommandOptionKind,
    name: String,
    description: String,
    required: Option<bool>,
    autocomplete: Option<bool>,
    min_value: Option<Value>,
    max_value: Option<Value>,
    channel_types: Vec<ChannelKind>,
    choices: Vec<(String, CommandOptionChoiceValue)>,
}

impl CommandOptionBuilder {
    /// Creates an option of the given kind.
    #[must_use]
    pub fn new(
        kind: CommandOptionKind,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            description: description.into(),
            required: None,
            autocomplete: None,
            min_value: None,
            max_value: None,
            channel_types: Vec::new(),
            choices: Vec::new(),
        }
    }

    /// Whether the option must be supplied by the user.
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Enables autocomplete for this option.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Structure, `autocomplete`) and `docs/application-commands.md`
    /// (Autocomplete) — autocomplete is only valid for string, integer and
    /// number options, and cannot be combined with choices.
    #[must_use]
    pub const fn autocomplete(mut self, autocomplete: bool) -> Self {
        self.autocomplete = Some(autocomplete);
        self
    }

    /// Appends one choice for a string or integer option.
    ///
    /// Choices serialise as the documented Application Command Option Choice
    /// Structure: `name` is clipped to 100 characters, a string `value` is
    /// clipped to 100 characters, and at most 25 choices are sent.
    ///
    /// An option cannot combine choices with autocomplete
    /// (`docs/application-commands.md`, Application Command Option Structure
    /// footnote), so [`Self::to_json`] omits `autocomplete` once choices exist.
    #[must_use]
    pub fn choice(
        mut self,
        name: impl Into<String>,
        value: impl Into<CommandOptionChoiceValue>,
    ) -> Self {
        self.choices.push((name.into(), value.into()));
        self
    }

    /// Sets the minimum value of an integer option.
    #[must_use]
    pub fn min_int_value(mut self, value: i64) -> Self {
        self.min_value = Some(Value::from(value));
        self
    }

    /// Sets the maximum value of an integer option.
    #[must_use]
    pub fn max_int_value(mut self, value: i64) -> Self {
        self.max_value = Some(Value::from(value));
        self
    }

    /// Sets the minimum value of a number option.
    #[must_use]
    pub fn min_number_value(mut self, value: f64) -> Self {
        self.min_value = Some(Value::from(value));
        self
    }

    /// Sets the maximum value of a number option.
    #[must_use]
    pub fn max_number_value(mut self, value: f64) -> Self {
        self.max_value = Some(Value::from(value));
        self
    }

    /// Restricts a channel option to the given channel kinds.
    ///
    /// The values are serialised as the numeric channel types listed in
    /// `docs/channel.md` (Channel Types).
    #[must_use]
    pub fn channel_types(mut self, types: Vec<ChannelKind>) -> Self {
        self.channel_types = types;
        self
    }

    /// Serialises the option object.
    ///
    /// Shape documented in `docs/application-commands.md` (Application Command Option Structure): `type` is the
    /// numeric option kind, `min_value`/`max_value` are JSON numbers.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("type".to_owned(), Value::from(self.kind.as_i64()));
        object.insert("name".to_owned(), Value::String(self.name.clone()));
        object.insert(
            "description".to_owned(),
            Value::String(self.description.clone()),
        );

        if let Some(required) = self.required {
            object.insert("required".to_owned(), Value::Bool(required));
        }

        if self.choices.is_empty() {
            if let Some(autocomplete) = self.autocomplete {
                object.insert("autocomplete".to_owned(), Value::Bool(autocomplete));
            }
        } else {
            let choices = self
                .choices
                .iter()
                .take(limits::COMMAND_CHOICES)
                .map(|(name, value)| {
                    let mut choice = Map::new();
                    choice.insert(
                        "name".to_owned(),
                        Value::String(limits::truncate(name, limits::COMMAND_CHOICE_NAME)),
                    );
                    choice.insert(
                        "value".to_owned(),
                        match value {
                            CommandOptionChoiceValue::String(value) => {
                                Value::String(limits::truncate(value, limits::COMMAND_CHOICE_VALUE))
                            }
                            CommandOptionChoiceValue::Integer(value) => Value::from(*value),
                        },
                    );
                    Value::Object(choice)
                })
                .collect();

            object.insert("choices".to_owned(), Value::Array(choices));
        }

        if let Some(min_value) = &self.min_value {
            object.insert("min_value".to_owned(), min_value.clone());
        }

        if let Some(max_value) = &self.max_value {
            object.insert("max_value".to_owned(), max_value.clone());
        }

        if !self.channel_types.is_empty() {
            let channel_types = self
                .channel_types
                .iter()
                .map(|kind| Value::from(kind.as_i64()))
                .collect();

            object.insert("channel_types".to_owned(), Value::Array(channel_types));
        }

        Value::Object(object)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::permissions::Permissions;

    use super::limits;
    use super::{CommandBuilder, CommandOptionBuilder};
    use crate::model::application::CommandOptionKind;

    #[test]
    fn slash_command_with_user_and_integer_options_and_default_permissions() {
        let command = CommandBuilder::new("ban", "Bans a user from the guild.")
            .default_member_permissions(Permissions::BAN_MEMBERS)
            .add_option(
                CommandOptionBuilder::new(CommandOptionKind::User, "target", "The member to ban.")
                    .required(true),
            )
            .add_option(
                CommandOptionBuilder::new(
                    CommandOptionKind::Integer,
                    "duration",
                    "Ban duration in days.",
                )
                .min_int_value(0)
                .max_int_value(7),
            );

        // BAN_MEMBERS is bit 2: the bitfield is sent as a decimal string.
        assert_eq!(
            command.to_json(),
            json!({
                "name": "ban",
                "description": "Bans a user from the guild.",
                "default_member_permissions": "4",
                "options": [
                    {
                        "type": 6,
                        "name": "target",
                        "description": "The member to ban.",
                        "required": true
                    },
                    {
                        "type": 4,
                        "name": "duration",
                        "description": "Ban duration in days.",
                        "min_value": 0,
                        "max_value": 7
                    }
                ]
            })
        );
    }

    #[test]
    fn option_kinds_serialise_to_their_documented_numbers() {
        let cases = [
            (CommandOptionKind::Subcommand, 1),
            (CommandOptionKind::SubcommandGroup, 2),
            (CommandOptionKind::Str, 3),
            (CommandOptionKind::Integer, 4),
            (CommandOptionKind::Boolean, 5),
            (CommandOptionKind::User, 6),
            (CommandOptionKind::Channel, 7),
            (CommandOptionKind::Role, 8),
            (CommandOptionKind::Mentionable, 9),
            (CommandOptionKind::Number, 10),
            (CommandOptionKind::Attachment, 11),
        ];

        for (kind, number) in cases {
            let option = CommandOptionBuilder::new(kind, "x", "y").to_json();

            assert_eq!(option["type"], json!(number), "wrong type for {kind:?}");
            assert_eq!(option.as_object().map(serde_json::Map::len), Some(3));
        }
    }

    #[test]
    fn number_bounds_are_json_numbers() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Number, "ratio", "A ratio.")
            .min_number_value(0.5)
            .max_number_value(2.5)
            .to_json();

        assert_eq!(
            option,
            json!({
                "type": 10,
                "name": "ratio",
                "description": "A ratio.",
                "min_value": 0.5,
                "max_value": 2.5
            })
        );
    }

    #[test]
    fn channel_types_use_the_documented_numbers() {
        use crate::model::channel::ChannelKind;

        let option = CommandOptionBuilder::new(CommandOptionKind::Channel, "channel", "Where.")
            .channel_types(vec![
                ChannelKind::GuildText,
                ChannelKind::GuildAnnouncement,
                ChannelKind::PublicThread,
            ])
            .to_json();

        assert_eq!(option["channel_types"], json!([0, 5, 11]));
    }

    #[test]
    fn flags_are_only_emitted_when_set() {
        let command = CommandBuilder::new("ping", "Ping!");

        assert_eq!(
            command.to_json(),
            json!({"name": "ping", "description": "Ping!"})
        );

        assert_eq!(
            CommandBuilder::new("ping", "Ping!")
                .dm_permission(false)
                .nsfw(true)
                .to_json(),
            json!({
                "name": "ping",
                "description": "Ping!",
                "dm_permission": false,
                "nsfw": true
            })
        );
    }

    #[test]
    fn options_beyond_the_documented_maximum_are_ignored() {
        let mut command = CommandBuilder::new("many", "Too many options.");

        for index in 0..30 {
            command = command.add_option(CommandOptionBuilder::new(
                CommandOptionKind::Str,
                format!("option{index}"),
                "An option.",
            ));
        }

        let json = command.to_json();
        let options = json["options"].as_array().expect("options array");

        assert_eq!(options.len(), 25);
        assert_eq!(options[24]["name"], json!("option24"));
    }

    #[test]
    fn autocomplete_is_serialised_only_when_enabled() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Str, "animal", "The animal.")
            .autocomplete(true)
            .to_json();

        assert_eq!(
            option,
            json!({
                "type": 3,
                "name": "animal",
                "description": "The animal.",
                "autocomplete": true
            })
        );

        assert_eq!(
            CommandOptionBuilder::new(CommandOptionKind::Str, "animal", "The animal.")
                .autocomplete(false)
                .to_json()["autocomplete"],
            json!(false)
        );

        assert!(
            CommandOptionBuilder::new(CommandOptionKind::Str, "animal", "The animal.")
                .to_json()
                .get("autocomplete")
                .is_none()
        );
    }

    #[test]
    fn min_and_max_are_independent_of_option_kind() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Integer, "count", "How many.")
            .min_int_value(-5)
            .max_int_value(5)
            .to_json();

        assert_eq!(option["min_value"], json!(-5));
        assert_eq!(option["max_value"], json!(5));
    }

    #[test]
    fn string_choices_serialise_the_documented_choice_objects() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Str, "execute", "Add or remove.")
            .choice("Add", "add")
            .choice("Remove", "remove")
            .to_json();

        assert_eq!(
            option,
            json!({
                "type": 3,
                "name": "execute",
                "description": "Add or remove.",
                "choices": [
                    {"name": "Add", "value": "add"},
                    {"name": "Remove", "value": "remove"}
                ]
            })
        );
    }

    #[test]
    fn integer_choices_serialise_as_json_numbers() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Integer, "count", "How many.")
            .choice("One", 1_i64)
            .to_json();

        assert_eq!(option["choices"], json!([{"name": "One", "value": 1}]));
    }

    #[test]
    fn choices_are_capped_at_the_documented_maximum() {
        let mut option =
            CommandOptionBuilder::new(CommandOptionKind::Integer, "count", "How many.");

        for index in 0..30 {
            option = option.choice(format!("Choice {index}"), index);
        }

        let json = option.to_json();
        let choices = json["choices"].as_array().expect("a choices array");

        assert_eq!(choices.len(), limits::COMMAND_CHOICES);
        assert_eq!(choices[0]["name"], json!("Choice 0"));
        assert_eq!(choices[24]["name"], json!("Choice 24"));
    }

    #[test]
    fn choices_take_precedence_over_autocomplete() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Str, "execute", "Add or remove.")
            .autocomplete(true)
            .choice("Add", "add")
            .to_json();

        assert!(option.get("autocomplete").is_none());
        assert!(option.get("choices").is_some());
    }

    #[test]
    fn choice_names_and_values_are_clipped() {
        let option = CommandOptionBuilder::new(CommandOptionKind::Str, "text", "Pick.")
            .choice("a".repeat(120), "b".repeat(120))
            .to_json();

        let name = option["choices"][0]["name"].as_str().unwrap_or_default();
        let value = option["choices"][0]["value"].as_str().unwrap_or_default();

        assert_eq!(name.chars().count(), limits::COMMAND_CHOICE_NAME);
        assert_eq!(value.chars().count(), limits::COMMAND_CHOICE_VALUE);
    }
}

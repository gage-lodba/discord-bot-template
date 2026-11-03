//! Helpers for reading slash command options by name.
//!
//! Discord only sends the options the user actually filled in, so options must
//! be looked up by name rather than by position. Reading `options.get(1)` for an
//! optional parameter silently picks up a *different* option when the earlier
//! one is omitted, and panics when the list is shorter than expected.
//!
//! This module also holds the fail-closed permission gate shared by the command
//! and component interaction handlers.
//!
//! Reference: `docs/receiving-and-responding.md` (Application Command Data
//! Structure, Message Component Data Structure), `docs/application-commands.md`
//! (Application Command Option Structure) and `docs/permissions.md` (Permission
//! Hierarchy, Permission Overwrites).

use discord::client::Context;
use discord::id::ChannelId;
use discord::id::GuildId;
use discord::id::UserId;
use discord::model::application::CommandDataOption;
use discord::model::application::CommandInteraction;
use discord::model::application::ComponentInteraction;
use discord::model::guild::Member;
use discord::permissions::Permissions;

use crate::error::AppError;

/// Finds an option by name in a raw option slice.
#[must_use]
pub(crate) fn find<'a>(
    options: &'a [CommandDataOption],
    name: &str,
) -> Option<&'a CommandDataOption> {
    options.iter().find(|option| option.name == name)
}

/// Returns a required string option from a raw option slice.
///
/// # Errors
///
/// [`AppError::MissingOption`] when absent, [`AppError::InvalidOption`] when it
/// is not a string.
pub(crate) fn string_of<'a>(
    options: &'a [CommandDataOption],
    name: &'static str,
) -> Result<&'a str, AppError> {
    let option = find(options, name).ok_or(AppError::MissingOption(name))?;

    option.value.as_str().ok_or(AppError::InvalidOption {
        option: name,
        expected: "text",
    })
}

/// Returns an optional string option from a raw option slice.
///
/// # Errors
///
/// [`AppError::InvalidOption`] when present but not a string.
pub(crate) fn optional_string_of<'a>(
    options: &'a [CommandDataOption],
    name: &'static str,
    default: &'a str,
) -> Result<&'a str, AppError> {
    match find(options, name) {
        Some(option) => option.value.as_str().ok_or(AppError::InvalidOption {
            option: name,
            expected: "text",
        }),
        None => Ok(default),
    }
}

/// Returns an optional integer option from a raw option slice.
///
/// # Errors
///
/// [`AppError::InvalidOption`] when present but not an integer.
pub(crate) fn integer_of(
    options: &[CommandDataOption],
    name: &'static str,
    default: i64,
) -> Result<i64, AppError> {
    match find(options, name) {
        Some(option) => option.value.as_i64().ok_or(AppError::InvalidOption {
            option: name,
            expected: "a whole number",
        }),
        None => Ok(default),
    }
}

/// Returns a required user option from a raw option slice.
///
/// # Errors
///
/// [`AppError::MissingOption`] when absent, [`AppError::InvalidOption`] when it
/// is not a user.
pub(crate) fn user_of(
    options: &[CommandDataOption],
    name: &'static str,
) -> Result<UserId, AppError> {
    let option = find(options, name).ok_or(AppError::MissingOption(name))?;

    option.value.as_user_id().ok_or(AppError::InvalidOption {
        option: name,
        expected: "a user",
    })
}

/// Returns the raw option slice, for helpers that work on options directly.
#[must_use]
pub(crate) fn options_of(interaction: &CommandInteraction) -> &[CommandDataOption] {
    &interaction.data.options
}

/// Returns a required string option from an interaction.
///
/// # Errors
///
/// See [`string_of`].
pub(crate) fn required_string<'a>(
    interaction: &'a CommandInteraction,
    name: &'static str,
) -> Result<&'a str, AppError> {
    string_of(options_of(interaction), name)
}

/// Returns an optional string option from an interaction.
///
/// # Errors
///
/// See [`optional_string_of`].
pub(crate) fn optional_string<'a>(
    interaction: &'a CommandInteraction,
    name: &'static str,
    default: &'a str,
) -> Result<&'a str, AppError> {
    optional_string_of(&interaction.data.options, name, default)
}

/// Returns an optional integer option from an interaction.
///
/// # Errors
///
/// See [`integer_of`].
pub(crate) fn optional_integer(
    interaction: &CommandInteraction,
    name: &'static str,
    default: i64,
) -> Result<i64, AppError> {
    integer_of(&interaction.data.options, name, default)
}

/// Returns a required user option from an interaction.
///
/// # Errors
///
/// See [`user_of`].
pub(crate) fn required_user(
    interaction: &CommandInteraction,
    name: &'static str,
) -> Result<UserId, AppError> {
    user_of(&interaction.data.options, name)
}

/// Checks that the invoking member holds `required` in the channel the
/// interaction was triggered in.
///
/// This is the shared implementation behind [`ensure_permission`] and
/// [`ensure_component_permission`]: a command and the buttons it produces must
/// be authorized by exactly the same rules.
///
/// # Errors
///
/// [`AppError::Denied`] when the member lacks the permission, the interaction
/// came from a DM, or the guild/channel/member is not cached yet;
/// [`AppError::Discord`] when permissions cannot be resolved.
fn ensure_member_permission(
    ctx: &Context,
    member: Option<&Member>,
    guild_id: Option<GuildId>,
    channel_id: Option<ChannelId>,
    required: Permissions,
) -> Result<(), AppError> {
    let Some(member) = member else {
        return Err(AppError::Denied("This can only be used inside a server."));
    };

    let guild_id = guild_id.ok_or(AppError::Denied("This can only be used inside a server."))?;

    let channel_id =
        channel_id.ok_or(AppError::Denied("This can only be used inside a server."))?;

    // Resolve through the guild so role and channel overwrites are available.
    // Threads live in `Guild::threads`, not in `Guild::channels`, so they are
    // checked separately: a command run inside a thread must not be denied just
    // because it is not a text channel.
    //
    // Failing closed here is deliberate: a permission check must never default to
    // "allowed" just because the cache is cold.
    let guild = ctx.cache.guild(guild_id).ok_or(AppError::Denied(
        "The server is still loading, please try again in a moment.",
    ))?;

    let channel = guild
        .channels
        .get(&channel_id)
        .or_else(|| guild.threads.iter().find(|thread| thread.id == channel_id))
        .ok_or(AppError::Denied(
            "This channel is still loading, please try again in a moment.",
        ))?;

    let permissions = guild.permissions_for(member, channel);

    if permissions.contains(required) || permissions.is_administrator() {
        return Ok(());
    }

    Err(AppError::Denied(
        "You do not have permission to use this command.",
    ))
}

/// Checks that the invoking member holds `required`.
///
/// `default_member_permissions` on the command registration is only a *default*:
/// guilds can override it with per-command permission overwrites, so it is not
/// an authorization boundary. This is the actual check, and it runs before any
/// destructive call.
///
/// Uses the guild's documented permission hierarchy
/// (`docs/permissions.md`), so role and channel overwrites are taken into
/// account, and an owner or administrator is allowed through.
///
/// # Errors
///
/// [`AppError::Denied`] when the member lacks the permission or the command was
/// used outside a guild, [`AppError::Discord`] when permissions cannot be
/// resolved.
pub(crate) fn ensure_permission(
    ctx: &Context,
    interaction: &CommandInteraction,
    required: Permissions,
) -> Result<(), AppError> {
    ensure_member_permission(
        ctx,
        interaction.member.as_ref(),
        interaction.guild_id,
        interaction.channel_id,
        required,
    )
}

/// Checks that the member who clicked a component holds `required`.
///
/// Component handlers re-check permissions instead of trusting the state of the
/// message they are attached to: a button can outlive the member's role, and a
/// cached message must never be an authorization boundary.
///
/// # Errors
///
/// See [`ensure_permission`]; a component interaction is authorized by exactly
/// the same fail-closed rules.
pub(crate) fn ensure_component_permission(
    ctx: &Context,
    interaction: &ComponentInteraction,
    required: Permissions,
) -> Result<(), AppError> {
    ensure_member_permission(
        ctx,
        interaction.member.as_ref(),
        interaction.guild_id,
        interaction.channel_id,
        required,
    )
}

#[cfg(test)]
mod tests {
    use discord::id::UserId;
    use discord::model::application::CommandDataOption;
    use serde_json::json;

    use super::*;

    /// Builds an option the way Discord delivers it.
    ///
    /// `CommandDataOption` is deliberately deserialised from the documented wire
    /// shape (`{ "name", "type", "value" }`), so these tests exercise exactly
    /// what the gateway sends rather than a hand-built struct literal.
    fn option(name: &str, kind: u8, value: &serde_json::Value) -> CommandDataOption {
        let raw = json!({ "name": name, "type": kind, "value": value });

        serde_json::from_value(raw).expect("option matches Discord's payload shape")
    }

    fn integer(name: &str, value: i64) -> CommandDataOption {
        option(name, 4, &json!(value))
    }

    fn string(name: &str, value: &str) -> CommandDataOption {
        option(name, 3, &json!(value))
    }

    fn user(name: &str, value: u64) -> CommandDataOption {
        // Discord sends a snowflake as a string.
        option(name, 6, &json!(value.to_string()))
    }

    #[test]
    fn options_deserialize_into_the_expected_variants() {
        // Guards the helpers below, so a shape change fails loudly here rather
        // than silently turning every later assertion into a type error.
        assert_eq!(integer("d", 7).value.as_i64(), Some(7));
        assert_eq!(string("r", "why").value.as_str(), Some("why"));
        assert_eq!(user("t", 42).value.as_user_id(), Some(UserId::new(42)));
    }

    #[test]
    fn finds_option_by_name_not_position() {
        // The regression this guards: `reason` is present while the earlier
        // optional `duration` is absent, so positional access would read the
        // reason as the duration and drop it.
        let options = vec![user("target", 42), string("reason", "spamming")];

        assert_eq!(
            integer_of(&options, "duration", 0).expect("absent duration falls back"),
            0
        );
        assert_eq!(
            optional_string_of(&options, "reason", "No reason provided")
                .expect("reason is present"),
            "spamming"
        );
    }

    #[test]
    fn missing_required_option_is_an_error_not_a_panic() {
        // `/ban target:@x` with no optional arguments used to panic here.
        let options = vec![user("target", 42)];

        let err = string_of(&options, "reason").expect_err("reason is absent");
        assert!(matches!(err, AppError::MissingOption("reason")));
        assert_eq!(err.to_string(), "missing required option 'reason'");
    }

    #[test]
    fn wrong_type_is_reported_as_invalid() {
        let options = vec![integer("target", 7), string("reason", "because")];

        let err = user_of(&options, "target").expect_err("target is an integer, not a user");
        match err {
            AppError::InvalidOption { option, expected } => {
                assert_eq!(option, "target");
                assert_eq!(expected, "a user");
            }
            other => panic!("expected InvalidOption, got {other:?}"),
        }
    }

    #[test]
    fn role_options_deserialize_into_role_ids() {
        // `/role` reads its `role` option from the raw slice, so the helper test
        // below stays honest only if the wire shape maps to a role id.
        let role = option("role", 8, &json!("99"));
        assert_eq!(role.value.as_role_id(), Some(discord::id::RoleId::new(99)));
        assert_eq!(role.value.as_user_id(), None);
    }

    #[test]
    fn optional_helpers_prefer_supplied_values() {
        let options = vec![integer("duration", 7), string("reason", "raid")];

        assert_eq!(
            integer_of(&options, "duration", 0).expect("duration is present"),
            7
        );
        assert_eq!(
            optional_string_of(&options, "reason", "No reason provided")
                .expect("reason is present"),
            "raid"
        );
    }

    #[test]
    fn required_user_returns_the_id() {
        let options = vec![user("target", 1234)];

        assert_eq!(
            user_of(&options, "target").expect("target is present"),
            UserId::new(1234)
        );
    }
}

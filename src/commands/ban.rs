use discord::builder::command::CommandBuilder;
use discord::builder::command::CommandOptionBuilder;
use discord::builder::interaction::InteractionEditBuilder;
use discord::client::Context;
use discord::id::GuildId;
use discord::id::UserId;
use discord::model::application::CommandInteraction;
use discord::model::application::CommandOptionKind;
use discord::permissions::Permissions;

use tracing::info;

use crate::error::AppError;
use crate::options::ensure_permission;
use crate::options::optional_integer;
use crate::options::optional_string;
use crate::options::required_user;

/// Maximum number of days of the target's recent messages a ban may delete.
///
/// Reference: `docs/guild.md` (Create Guild Ban: `delete_message_seconds` is
/// 0-604800, i.e. 0-7 days).
const MAX_DELETE_MESSAGE_DAYS: i64 = 7;

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // `default_member_permissions` in `register()` is only a default that a
    // guild can override, so the real permission is checked before anything is
    // acknowledged or changed. This runs first so a denial can still be shown
    // through the normal (non-deferred) error reply.
    ensure_permission(ctx, interaction, Permissions::BAN_MEMBERS)?;

    // Discord only sends the options the user actually filled in, so they are
    // looked up by name. Positional access used to panic on `/ban target:@x`
    // and to read `reason` as the duration when `duration` was omitted.
    let target: UserId = required_user(interaction, "target")?;
    let duration: i64 = optional_integer(interaction, "duration", 0)?;
    let reason: &str = optional_string(interaction, "reason", "No reason provided")?;

    // Validated before the defer so a bad value is reported through the ordinary
    // reply path. `delete_message_seconds` only controls how much of the target's
    // recent message history is purged; the ban itself is indefinite. The option
    // is registered 0..=7, but the value is still range-checked rather than
    // silently truncated by a cast.
    let delete_message_days = u8::try_from(duration)
        .ok()
        .filter(|days| i64::from(*days) <= MAX_DELETE_MESSAGE_DAYS)
        .ok_or(AppError::Denied(
            "The duration must be a whole number of days between 0 and 7.",
        ))?;

    // Acknowledge before the REST calls below: Discord invalidates the token if
    // the first response does not arrive within 3 seconds. The reply stays
    // ephemeral, exactly as it was before.
    interaction.defer_ephemeral(&ctx.http).await?;

    // Copy the id out of the cache before awaiting: the cache lookup returns an
    // owned snapshot, so nothing is held across the request.
    let bot_id: UserId = ctx
        .cache
        .current_user()
        .ok_or(AppError::Denied(
            "The bot is still starting up, please try again in a moment.",
        ))?
        .id;

    if target == bot_id {
        let response = InteractionEditBuilder::new().content("Cannot ban myself sorry. :)");
        interaction.edit_response(&ctx.http, response).await?;

        return Ok(());
    }

    // Logged by id rather than fetching the user just to print a name: this is a
    // whole Discord round trip on the critical path, and the id is enough to
    // identify the target in the audit log written by the audit-log handler.
    info!("Target: {}", target);
    info!("Duration: {}", duration);
    info!("Reason: {}", reason);

    // Use the guild the command was invoked in, not a hard-coded literal: the
    // permission check above resolved the member against this same guild, so
    // banning elsewhere would check one server and act on another.
    let guild_id: GuildId = interaction.guild_id.ok_or(AppError::Denied(
        "This command can only be used inside a server.",
    ))?;

    ctx.http
        .ban_member(guild_id, target, delete_message_days, Some(reason))
        .await?;

    info!("Received ban command from {}.", interaction.user.username);

    let response = InteractionEditBuilder::new().content("Ban command executed!");
    interaction.edit_response(&ctx.http, response).await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("ban", "Bans a user from the guild.")
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::User,
                "target",
                "Bans the user from the guild.",
            )
            .required(true),
        )
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::Integer,
                "duration",
                "Days of the target's recent messages to delete (0-7). The ban does not expire.",
            )
            .min_int_value(0)
            .max_int_value(7),
        )
        .add_option(CommandOptionBuilder::new(
            CommandOptionKind::Str,
            "reason",
            "The reason for the ban.",
        ))
        .default_member_permissions(Permissions::BAN_MEMBERS)
}

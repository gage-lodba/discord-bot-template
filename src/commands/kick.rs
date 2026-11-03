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
use crate::options::optional_string;
use crate::options::required_user;

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // `default_member_permissions` in `register()` is only a default that a
    // guild can override, so the real permission is checked before anything is
    // acknowledged or changed. This runs first so a denial can still be shown
    // through the normal (non-deferred) error reply.
    ensure_permission(ctx, interaction, Permissions::KICK_MEMBERS)?;

    // Options are read by name: the old positional access read `reason` as the
    // target when the user only supplied one option.
    let target: UserId = required_user(interaction, "target")?;
    let reason: &str = optional_string(interaction, "reason", "No reason provided")?;

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
        let response = InteractionEditBuilder::new().content("Cannot kick myself sorry. :)");
        interaction.edit_response(&ctx.http, response).await?;

        return Ok(());
    }

    // Logged by id rather than fetching the user just to print a name: this is a
    // whole Discord round trip on the critical path.
    info!("Kick target: {}", target);
    info!("Kick reason: {}", reason);

    // Use the guild the command was invoked in, not a hard-coded literal: the
    // permission check above resolved the member against this same guild, so
    // kicking elsewhere would check one server and act on another.
    let guild_id: GuildId = interaction.guild_id.ok_or(AppError::Denied(
        "This command can only be used inside a server.",
    ))?;

    ctx.http.kick_member(guild_id, target, Some(reason)).await?;

    info!(
        "Received kick command from {}[{}].",
        interaction.user.username, interaction.user.id
    );

    let response = InteractionEditBuilder::new().content("Kick command executed!");
    interaction.edit_response(&ctx.http, response).await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("kick", "Kicks a user from the guild.")
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::User,
                "target",
                "The user you want to kick.",
            )
            .required(true),
        )
        .add_option(CommandOptionBuilder::new(
            CommandOptionKind::Str,
            "reason",
            "Why they are being kicked.",
        ))
        .default_member_permissions(Permissions::KICK_MEMBERS)
}

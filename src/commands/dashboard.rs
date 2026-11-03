use discord::builder::command::CommandBuilder;
use discord::builder::embed::EmbedBuilder;
use discord::builder::interaction::InteractionEditBuilder;
use discord::builder::message::MessageBuilder;
use discord::client::Context;
use discord::id::ChannelId;
use discord::model::application::CommandInteraction;
use discord::permissions::Permissions;

use tracing::info;

use crate::config::Dashboard;
use crate::error::AppError;
use crate::options::ensure_permission;

/// The rules content posted by `/dashboard`.
const RULES: &str = "# 📜 Server Rules\n- Follow the discord [terms](https://discord.com/terms) and [guidelines](https://discord.com/guidelines).\n- Don't be stupid.";

/// Reads the configured dashboard content out of the client's data map.
///
/// # Errors
///
/// [`AppError::Denied`] when the data map has no [`Dashboard`] entry.
async fn configured_dashboard(ctx: &Context) -> Result<Dashboard, AppError> {
    ctx.data
        .read()
        .await
        .get::<Dashboard>()
        .cloned()
        .ok_or(AppError::Denied(
            "The bot's configuration is not available.",
        ))
}

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // As with ban and kick, `default_member_permissions` in `register()` is only
    // an overridable default, so the real permission is checked here. This runs
    // before the defer so a denial is still shown through the ordinary
    // (non-deferred) error reply.
    ensure_permission(ctx, interaction, Permissions::ADMINISTRATOR)?;

    // Acknowledge before sending the embeds: that send is a REST call, and
    // Discord invalidates the token if the first response does not arrive
    // within 3 seconds. The reply stays ephemeral, exactly as it was before.
    interaction.defer_ephemeral(&ctx.http).await?;

    let dashboard = configured_dashboard(ctx).await?;

    let rules_embed = EmbedBuilder::new().color(0x00ff_ffff).description(RULES);

    // The invite embed is deployment-specific: there is no default invite, so it
    // is only posted when `DISCORD_INVITE_URL` is configured.
    let mut embeds = vec![rules_embed.build()];

    if let Some(invite_url) = &dashboard.invite_url {
        let invite_embed = EmbedBuilder::new()
            .color(0x00ff_ffff)
            .description(format!("# 🔗 Invite Link\n{invite_url}"));

        embeds.push(invite_embed.build());
    }

    let builder = MessageBuilder::new().embeds(embeds);

    let channel: ChannelId = interaction.channel_id.ok_or(AppError::Denied(
        "This command can only be used inside a channel.",
    ))?;

    channel.send_message(&ctx.http, &builder).await?;

    info!(
        "Received dashboard command from {}.",
        interaction.user.username
    );

    let response = InteractionEditBuilder::new().content("Dashboard sent!");
    interaction.edit_response(&ctx.http, response).await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("dashboard", "Sends the dashboard to the channel.")
        .default_member_permissions(Permissions::ADMINISTRATOR)
}

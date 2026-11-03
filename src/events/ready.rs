use discord::builder::command::CommandBuilder;
use discord::builder::presence::Activity;
use discord::builder::presence::PresenceBuilder;
use discord::builder::presence::Status;
use discord::client::Context;
use discord::id::GuildId;
use discord::model::ready::Ready;

use tracing::error;
use tracing::info;

use crate::commands;
use crate::config::Ids;

pub(crate) async fn ready(ctx: Context, _data_about_bot: Ready, ids: Ids) {
    // Guild-scoped registration means these commands exist in the configured
    // guild only.
    // Reference: `docs/application-commands.md` (Bulk Overwrite Guild
    // Application Commands) and `docs/application.md` (Application Object).
    let guild_id: GuildId = ids.guild;

    let builders: Vec<CommandBuilder> = vec![
        // fun
        commands::say::register(),
        // Utility
        commands::info::register(),
        commands::ping::register(),
        // Administration
        commands::ban::register(),
        commands::kick::register(),
        commands::role::register(),
        // Server Owner
        commands::dashboard::register(),
    ];

    let registered = guild_id.set_commands(&ctx.http, &builders).await;

    let commands: Vec<discord::model::application::Command> = match registered {
        Ok(commands) => commands,
        Err(err) => {
            error!("failed to register guild commands in {}: {}", guild_id, err);
            return;
        }
    };

    info!("{} Commands registered.", commands.len());

    let presence = PresenceBuilder::new()
        .status(Status::Online)
        .activity(Activity::custom("Playing with code."));

    if let Err(err) = ctx.update_presence(&presence) {
        // A failed presence update is cosmetic; the bot still serves commands,
        // so it is reported rather than fatal.
        error!("failed to set presence: {err}");
    }

    info!("Bot is connected!");
}

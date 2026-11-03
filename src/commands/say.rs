use discord::builder::command::CommandBuilder;
use discord::builder::command::CommandOptionBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::model::application::CommandInteraction;
use discord::model::application::CommandOptionKind;
use discord::permissions::Permissions;

use tracing::info;

use crate::error::AppError;
use crate::options::ensure_permission;
use crate::options::required_string;

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // The bot speaks here under its own identity, and the message is echoed
    // verbatim — so without this check any member could broadcast arbitrary
    // text as the bot, including a phishing link or a user mention that pings
    // on their behalf.
    //
    // Only user mentions ping in an interaction response: the default
    // `allowed_mentions` for interactions parses `users` only
    // (`docs/message.md`, Default Settings for Allowed Mentions), so
    // `@everyone` and role mentions in the echoed text notify nobody, while a
    // user mention does notify. The permission gate is what bounds that.
    ensure_permission(ctx, interaction, Permissions::MANAGE_MESSAGES)?;

    // Reading by name instead of position means a missing or wrong-typed option
    // becomes a reported error rather than a panic.
    let message: &str = required_string(interaction, "message")?;

    info!("Received say command from {}.", interaction.user.username);
    interaction
        .respond(&ctx.http, ReplyBuilder::new().content(message))
        .await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("say", "Says a message through the bot.")
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::Str,
                "message",
                "This message will be said by the bot.",
            )
            .required(true),
        )
        // Only a default: guilds can override it with command permission
        // overwrites, which is why `run` enforces the real check above.
        .default_member_permissions(Permissions::MANAGE_MESSAGES)
}

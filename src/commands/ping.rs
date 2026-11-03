use discord::builder::command::CommandBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::model::application::CommandInteraction;

use tracing::info;

use crate::error::AppError;

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    let response = ReplyBuilder::new().content("Pong!").ephemeral();

    info!("Received ping command from {}.", interaction.user.username);
    interaction.respond(&ctx.http, response).await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("ping", "Replies with pong to test latency.")
}

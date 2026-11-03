//! Routing for `INTERACTION_CREATE`.
//!
//! Every documented interaction type is dispatched here:
//!
//! | Type | Route |
//! | - | - |
//! | `PING` (`1`) | ignored here: the crate's gateway dispatch already acknowledged it with a `PONG` |
//! | `APPLICATION_COMMAND` (`2`) | the command handlers in [`crate::commands`] |
//! | `MESSAGE_COMPONENT` (`3`) | [`crate::events::components`] |
//! | `APPLICATION_COMMAND_AUTOCOMPLETE` (`4`) | [`crate::events::autocomplete`] |
//! | `MODAL_SUBMIT` (`5`) | [`crate::events::modal_submit`] |
//! | unknown | ignored at `debug!` level so a new Discord type cannot break the bot |
//!
//! Handlers must answer every interaction they accept: an unanswered interaction
//! shows the user "This interaction failed". The command path reports failures
//! through [`crate::error::report`], and the component path through
//! [`crate::error::report_component`]; autocomplete answers with an empty list
//! rather than failing when it cannot build choices.
//!
//! Reference: `docs/receiving-and-responding.md` (Interaction Object, Interaction
//! Type, Interaction Callback) and `docs/interactions-overview.md`.

use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::model::application::CommandInteraction;
use discord::model::application::Interaction;

use tracing::debug;
use tracing::error;
use tracing::info;

use crate::commands;
use crate::events::autocomplete;
use crate::events::components;
use crate::events::modal_submit;

/// Routes one parsed interaction to its handler.
pub(crate) async fn interaction_create(ctx: Context, interaction: Interaction) {
    // Remote payloads decide which branch runs, so an unrecognised interaction
    // type is ignored instead of treated as a command.
    match &interaction {
        Interaction::Command(command) => handle_command(&ctx, command).await,
        Interaction::Component(component) => components::component(&ctx, component).await,
        Interaction::Autocomplete(autocomplete_interaction) => {
            autocomplete::autocomplete(&ctx, autocomplete_interaction).await;
        }
        Interaction::ModalSubmit(modal) => modal_submit::modal_submit(&ctx, modal).await,
        Interaction::Ping(ping) => {
            // The crate acknowledges PING interactions itself before this
            // handler runs (`discord::gateway::dispatch`); there is nothing left
            // to answer. Reaching this arm at all is only interesting for
            // debugging.
            debug!(interaction = %ping.id, "ignoring a PING interaction; it was answered with a PONG");
        }
        Interaction::Unknown(kind) => {
            debug!(
                kind,
                "ignoring an interaction type this bot does not handle"
            );
        }
    }
}

/// Dispatches an `APPLICATION_COMMAND` interaction to its command handler.
async fn handle_command(ctx: &Context, command: &CommandInteraction) {
    // Only report success once the command actually succeeded. Logging
    // "Processed command" unconditionally made a failed command look like a
    // success in the log, right next to the failure it had just recorded.
    let outcome: Result<(), crate::error::AppError> = match command.data.name.as_str() {
        "say" => commands::say::run(ctx, command).await,
        "info" => commands::info::run(ctx, command).await,
        "ping" => commands::ping::run(ctx, command).await,
        "ban" => commands::ban::run(ctx, command).await,
        "kick" => commands::kick::run(ctx, command).await,
        "role" => commands::role::run(ctx, command).await,
        "dashboard" => commands::dashboard::run(ctx, command).await,

        other => {
            error!("Unknown command: {other}");

            let response = ReplyBuilder::new().content("Sorry, I don't recognize that command.");

            command
                .respond(&ctx.http, response)
                .await
                .map_err(crate::error::AppError::from)
        }
    };

    match outcome {
        Ok(()) => {
            info!(
                "Processed command '{}' from {}[{}]",
                command.data.name, command.user.username, command.user.id
            );
        }
        Err(err) => {
            crate::error::report(
                ctx,
                command,
                &format!("failed to run /{}", command.data.name),
                err,
            )
            .await;
        }
    }
}

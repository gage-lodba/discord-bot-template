//! Routing for message component interactions.
//!
//! The bot currently exposes one interactive component: the **Undo** button on
//! the ephemeral `/role` result (see [`crate::commands::role`]). Everything else
//! is either a button from a previous version of the bot or an interaction from
//! a component this build does not know about. Those are answered with an
//! ephemeral explanation instead of being ignored, because an unanswered
//! component makes Discord show the user "This interaction failed".
//!
//! Every handler runs after the crate has parsed and (for `PING` only)
//! acknowledged the interaction; answering remains this module's job.
//!
//! Reference: `docs/receiving-and-responding.md` (Message Component Data
//! Structure, Interaction Callback) and `docs/components-reference.md` (Anatomy
//! of a Component, Custom ID).

use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::model::application::ComponentInteraction;

use tracing::error;
use tracing::warn;

use crate::commands;
use crate::error::UNSUPPORTED_MESSAGE;
use crate::error::report_component;

/// Handles a `MESSAGE_COMPONENT` interaction.
pub(crate) async fn component(ctx: &Context, interaction: &ComponentInteraction) {
    // The prefix check keeps unrelated buttons on the generic fallback path: a
    // malformed `role-undo:` value is answered as unsupported rather than being
    // reported as a fault.
    if commands::role::is_undo_custom_id(&interaction.data.custom_id) {
        if let Err(err) = commands::role::undo(ctx, interaction).await {
            report_component(ctx, interaction, "failed to undo a /role change", err).await;
        }

        return;
    }

    warn!(
        custom_id = %interaction.data.custom_id,
        "ignoring a component interaction with no registered handler"
    );

    let response = ReplyBuilder::new().content(UNSUPPORTED_MESSAGE).ephemeral();

    if let Err(err) = interaction.respond(&ctx.http, response).await {
        error!(
            custom_id = %interaction.data.custom_id,
            %err,
            "failed to answer an unhandled component interaction"
        );
    }
}

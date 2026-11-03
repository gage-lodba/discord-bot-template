//! Routing for modal submit interactions.
//!
//! The bot does not open any modals yet, so a `MODAL_SUBMIT` can only come from
//! a stale or edited component. It is still answered (with an ephemeral
//! explanation) so the user never sees Discord's "This interaction failed", and
//! logged at `warn!` so the operator can see that a `custom_id` has no handler.
//!
//! Reference: `docs/receiving-and-responding.md` (Modal Submit Data Structure,
//! Interaction Callback Type, `MODAL`) and `docs/using-modal-components.md`.

use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::model::application::ModalSubmitInteraction;

use tracing::error;
use tracing::warn;

use crate::error::UNSUPPORTED_MESSAGE;

/// Handles a `MODAL_SUBMIT` interaction.
pub(crate) async fn modal_submit(ctx: &Context, interaction: &ModalSubmitInteraction) {
    warn!(
        custom_id = %interaction.data.custom_id,
        "ignoring a modal submit interaction with no registered handler"
    );

    let response = ReplyBuilder::new().content(UNSUPPORTED_MESSAGE).ephemeral();

    if let Err(err) = interaction.respond(&ctx.http, response).await {
        error!(
            custom_id = %interaction.data.custom_id,
            %err,
            "failed to answer an unhandled modal submit interaction"
        );
    }
}

//! Routing for autocomplete interactions.
//!
//! Autocomplete interactions must be answered within 3 seconds
//! (`docs/receiving-and-responding.md`, Interaction Callback), so the handler is
//! cache-only: it never performs a REST call before responding. A failure to
//! produce choices is answered with an empty list rather than left unanswered,
//! which is a valid response and keeps the picker responsive.
//!
//! Reference: `docs/application-commands.md` (Autocomplete) and
//! `docs/receiving-and-responding.md` (Application Command Data Structure,
//! `focused`).

use discord::builder::interaction::AutocompleteBuilder;
use discord::client::Context;
use discord::model::application::AutocompleteInteraction;

use tracing::error;
use tracing::warn;

use crate::commands;

/// Handles an `APPLICATION_COMMAND_AUTOCOMPLETE` interaction.
pub(crate) async fn autocomplete(ctx: &Context, interaction: &AutocompleteInteraction) {
    let choices = if interaction.data.name == "role" {
        commands::role::autocomplete_choices(ctx, interaction)
    } else {
        warn!(
            command = %interaction.data.name,
            "ignoring an autocomplete interaction for a command with no handler"
        );

        Vec::new()
    };

    let response = AutocompleteBuilder::new().choices(choices);

    if let Err(err) = interaction.respond_choices(&ctx.http, response).await {
        error!(
            command = %interaction.data.name,
            %err,
            "failed to answer an autocomplete interaction"
        );
    }
}

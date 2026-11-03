//! Convenience methods the bot calls on model types.
//!
//! These are thin wrappers around [`HttpClient`] that keep call sites short:
//! `channel.send_message(&ctx.http, &message)`,
//! `interaction.respond(&ctx.http, reply)` and so on. The endpoint behaviour
//! (authentication, rate limits, retries) lives in [`super::client`].
//!
//! Interaction endpoints are unauthenticated: the interaction token in the path
//! is the credential and the requests are exempt from the bot's global rate
//! limit (`docs/rate-limits.md`, Global Rate Limit).
//!
//! Reference: `docs/receiving-and-responding.md` (Interaction Callback, Followup
//! Messages, Endpoints).

use super::client::HttpClient;
use super::routes;
use crate::builder::command::CommandBuilder;
use crate::builder::interaction::{
    AutocompleteBuilder, DeferBuilder, DeferUpdateBuilder, InteractionEditBuilder,
    InteractionResponse, ModalBuilder, UpdateMessageBuilder,
};
use crate::builder::message::MessageBuilder;
use crate::error::Error;
use crate::id::{ChannelId, GuildId, MessageId};
use crate::model::application::{
    AutocompleteInteraction, Command, CommandInteraction, ComponentInteraction,
    InteractionCallbackResponse, ModalSubmitInteraction, PingInteraction,
};
use crate::model::message::Message;

impl ChannelId {
    /// Posts a message to this channel.
    ///
    /// Reference: `docs/message.md` (Create Message).
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`HttpClient::send_message`]: [`Error::Http`]
    /// for a transport failure or a non-success response (including an exhausted
    /// rate limit), and [`Error::Json`] for a response body that is not a
    /// documented message object.
    pub async fn send_message(
        &self,
        http: &HttpClient,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        http.send_message(*self, message).await
    }
}

impl GuildId {
    /// Registers this guild's slash commands.
    ///
    /// The application id is taken from the client; it is only known after the
    /// gateway has delivered `READY`, so calling this before the bot is ready
    /// returns an error rather than sending a request with a missing id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] with a transport message when the application id
    /// is not known yet (no `READY`), for a transport failure or a non-success
    /// response (including an exhausted rate limit), and [`Error::Json`] for a
    /// response body that is not a documented array of command objects.
    pub async fn set_commands(
        &self,
        http: &HttpClient,
        commands: &[CommandBuilder],
    ) -> Result<Vec<Command>, Error> {
        let application_id = http
            .application_id()
            .ok_or_else(routes::missing_application_id)?;

        http.set_guild_commands(application_id, *self, commands)
            .await
    }
}

impl crate::id::ApplicationId {
    /// Registers this application's global slash commands.
    ///
    /// Global command changes can take a while to propagate to Discord clients;
    /// registering the same commands in a guild ([`GuildId::set_commands`]) is
    /// visible immediately, which is what a development setup usually wants.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] for a transport failure or a non-success response
    /// (including an exhausted rate limit), and [`Error::Json`] for a response
    /// body that is not a documented array of command objects.
    pub async fn set_global_commands(
        &self,
        http: &HttpClient,
        commands: &[CommandBuilder],
    ) -> Result<Vec<Command>, Error> {
        http.set_global_commands(*self, commands).await
    }
}

impl PingInteraction {
    /// Acknowledges this ping interaction with a pong callback.
    ///
    /// Reference: `docs/interactions-overview.md` (Acknowledging PING requests)
    /// and `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `PONG`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn ack(&self, http: &HttpClient) -> Result<(), Error> {
        http.create_interaction_response(self.id, &self.token, InteractionResponse::Pong)
            .await
    }
}

impl CommandInteraction {
    /// Sends an initial response to this interaction.
    ///
    /// Reference: `docs/rate-limits.md` (Global Rate Limit) — the interaction
    /// token authenticates the request, so no bot authorization is sent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond(
        &self,
        http: &HttpClient,
        response: impl Into<InteractionResponse>,
    ) -> Result<(), Error> {
        http.create_interaction_response(self.id, &self.token, response.into())
            .await
    }

    /// Sends an initial response and returns the interaction callback response
    /// object.
    ///
    /// The callback is sent with `with_response=true`, so Discord answers with
    /// the created message (when the callback creates one).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response, Query String Params).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented interaction
    /// callback response object.
    pub async fn respond_with_response(
        &self,
        http: &HttpClient,
        response: impl Into<InteractionResponse>,
    ) -> Result<InteractionCallbackResponse, Error> {
        http.create_interaction_response_with_response(self.id, &self.token, response.into())
            .await
    }

    /// Sends a modal as this interaction's response (callback type 9).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `MODAL`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond_modal(&self, http: &HttpClient, modal: ModalBuilder) -> Result<(), Error> {
        self.respond(http, modal).await
    }

    /// Acknowledges this interaction with a public "thinking" response.
    ///
    /// Use [`Self::edit_response`] to fill in the reply later.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn defer(&self, http: &HttpClient) -> Result<(), Error> {
        self.respond(http, DeferBuilder::new()).await
    }

    /// Acknowledges this interaction with an ephemeral "thinking" response,
    /// visible only to the invoking user.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn defer_ephemeral(&self, http: &HttpClient) -> Result<(), Error> {
        self.respond(http, DeferBuilder::new().ephemeral()).await
    }

    /// Edits the interaction's original response.
    ///
    /// Returns the updated message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Edit Original Interaction
    /// Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_response(
        &self,
        http: &HttpClient,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_interaction_response(self.application_id, &self.token, edit)
            .await
    }

    /// Fetches the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Original Interaction
    /// Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_response(&self, http: &HttpClient) -> Result<Message, Error> {
        http.get_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Deletes the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Original
    /// Interaction Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_response(&self, http: &HttpClient) -> Result<(), Error> {
        http.delete_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Sends a followup message to this interaction.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn followup(
        &self,
        http: &HttpClient,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        http.create_followup_message(self.application_id, &self.token, message)
            .await
    }

    /// Fetches one of this interaction's followup messages.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<Message, Error> {
        http.get_followup_message(self.application_id, &self.token, message_id)
            .await
    }

    /// Edits one of this interaction's followup messages.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Edit Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_followup_message(self.application_id, &self.token, message_id, edit)
            .await
    }

    /// Deletes one of this interaction's followup messages.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<(), Error> {
        http.delete_followup_message(self.application_id, &self.token, message_id)
            .await
    }
}

impl ComponentInteraction {
    /// Sends an initial response to this interaction.
    ///
    /// Use [`Self::defer_update`] to defer editing the message, or
    /// [`Self::update_message`] to edit it immediately.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond(
        &self,
        http: &HttpClient,
        response: impl Into<InteractionResponse>,
    ) -> Result<(), Error> {
        http.create_interaction_response(self.id, &self.token, response.into())
            .await
    }

    /// Acknowledges this interaction and edits the original message later
    /// (callback type 6).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `DEFERRED_UPDATE_MESSAGE`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn defer_update(&self, http: &HttpClient) -> Result<(), Error> {
        self.respond(http, DeferUpdateBuilder::new()).await
    }

    /// Edits the message the component was attached to (callback type 7).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `UPDATE_MESSAGE`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn update_message(
        &self,
        http: &HttpClient,
        update: UpdateMessageBuilder,
    ) -> Result<(), Error> {
        self.respond(http, update).await
    }

    /// Sends a modal as this interaction's response (callback type 9).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `MODAL`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond_modal(&self, http: &HttpClient, modal: ModalBuilder) -> Result<(), Error> {
        self.respond(http, modal).await
    }

    /// Edits the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Edit Original Interaction
    /// Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_response(
        &self,
        http: &HttpClient,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_interaction_response(self.application_id, &self.token, edit)
            .await
    }

    /// Fetches the interaction's original response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_response(&self, http: &HttpClient) -> Result<Message, Error> {
        http.get_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Deletes the interaction's original response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_response(&self, http: &HttpClient) -> Result<(), Error> {
        http.delete_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Sends a followup message to this interaction.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn followup(
        &self,
        http: &HttpClient,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        http.create_followup_message(self.application_id, &self.token, message)
            .await
    }

    /// Fetches one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<Message, Error> {
        http.get_followup_message(self.application_id, &self.token, message_id)
            .await
    }

    /// Edits one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_followup_message(self.application_id, &self.token, message_id, edit)
            .await
    }

    /// Deletes one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<(), Error> {
        http.delete_followup_message(self.application_id, &self.token, message_id)
            .await
    }
}

impl ModalSubmitInteraction {
    /// Sends an initial response to this interaction.
    ///
    /// A modal cannot be answered with another modal
    /// (`docs/receiving-and-responding.md`, Interaction Callback Type), so use
    /// a message reply, [`Self::defer_update`] or [`Self::update_message`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond(
        &self,
        http: &HttpClient,
        response: impl Into<InteractionResponse>,
    ) -> Result<(), Error> {
        http.create_interaction_response(self.id, &self.token, response.into())
            .await
    }

    /// Acknowledges this interaction and edits the original message later
    /// (callback type 6).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `DEFERRED_UPDATE_MESSAGE`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn defer_update(&self, http: &HttpClient) -> Result<(), Error> {
        self.respond(http, DeferUpdateBuilder::new()).await
    }

    /// Edits the message the modal was opened from (callback type 7).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `UPDATE_MESSAGE`).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn update_message(
        &self,
        http: &HttpClient,
        update: UpdateMessageBuilder,
    ) -> Result<(), Error> {
        self.respond(http, update).await
    }

    /// Edits the interaction's original response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_response(
        &self,
        http: &HttpClient,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_interaction_response(self.application_id, &self.token, edit)
            .await
    }

    /// Fetches the interaction's original response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_response(&self, http: &HttpClient) -> Result<Message, Error> {
        http.get_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Deletes the interaction's original response.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_response(&self, http: &HttpClient) -> Result<(), Error> {
        http.delete_interaction_response(self.application_id, &self.token)
            .await
    }

    /// Sends a followup message to this interaction.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn followup(
        &self,
        http: &HttpClient,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        http.create_followup_message(self.application_id, &self.token, message)
            .await
    }

    /// Fetches one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<Message, Error> {
        http.get_followup_message(self.application_id, &self.token, message_id)
            .await
    }

    /// Edits one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        http.edit_followup_message(self.application_id, &self.token, message_id, edit)
            .await
    }

    /// Deletes one of this interaction's followup messages.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_followup(
        &self,
        http: &HttpClient,
        message_id: MessageId,
    ) -> Result<(), Error> {
        http.delete_followup_message(self.application_id, &self.token, message_id)
            .await
    }
}

impl AutocompleteInteraction {
    /// Sends an initial response to this interaction.
    ///
    /// Autocomplete interactions expect the
    /// [`crate::builder::interaction::AutocompleteBuilder`] callback (type 8);
    /// prefer [`Self::respond_choices`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond(
        &self,
        http: &HttpClient,
        response: impl Into<InteractionResponse>,
    ) -> Result<(), Error> {
        http.create_interaction_response(self.id, &self.token, response.into())
            .await
    }

    /// Responds to this autocomplete interaction with choices (callback type 8).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
    /// `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT`) and
    /// `docs/application-commands.md` (Autocomplete).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn respond_choices(
        &self,
        http: &HttpClient,
        choices: AutocompleteBuilder,
    ) -> Result<(), Error> {
        self.respond(http, choices).await
    }
}

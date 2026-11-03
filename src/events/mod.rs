use std::future::Future;
use std::future::ready;

use discord::client::Context;
use discord::client::EventHandler;
use discord::id::GuildId;
use discord::model::application::Interaction;
use discord::model::audit_log::AuditLogEntry;
use discord::model::guild::Member;
use discord::model::message::Message;
use discord::model::ready::Ready;
use discord::model::ready::Resumed;
use discord::model::user::User;

use crate::config::Ids;

//
//  Events
//
pub(crate) mod autocomplete;
pub(crate) mod components;
pub(crate) mod guild_audit_log_entry_create;
pub(crate) mod guild_ban_addition;
pub(crate) mod guild_ban_removal;
pub(crate) mod guild_member_addition;
pub(crate) mod guild_member_removal;
pub(crate) mod interaction_create;
pub(crate) mod message;
pub(crate) mod modal_submit;
pub(crate) mod ready;
pub(crate) mod resume;

//
//  Structures
//

/// Gateway event handler.
///
/// Carries the configured [`Ids`] so handlers do not have to hard-code the guild
/// or the announcement channel. `Ids` is `Copy` and does not hold the token.
pub(crate) struct Handler {
    pub(crate) ids: Ids,
}

impl Handler {
    #[must_use]
    pub(crate) fn new(ids: Ids) -> Self {
        Self { ids }
    }
}

//
//  Implementations
//
impl EventHandler for Handler {
    async fn on_guild_audit_log_entry_create(
        &self,
        ctx: Context,
        entry: AuditLogEntry,
        guild_id: GuildId,
    ) {
        guild_audit_log_entry_create::guild_audit_log_entry_create(ctx, entry, guild_id, self.ids)
            .await;
    }

    fn on_guild_ban_add(
        &self,
        ctx: Context,
        guild_id: GuildId,
        banned_user: User,
    ) -> impl Future<Output = ()> + Send {
        guild_ban_addition::guild_ban_addition(ctx, guild_id, &banned_user);

        ready(())
    }

    fn on_guild_ban_remove(
        &self,
        ctx: Context,
        guild_id: GuildId,
        unbanned_user: User,
    ) -> impl Future<Output = ()> + Send {
        guild_ban_removal::guild_ban_removal(ctx, guild_id, &unbanned_user);

        ready(())
    }

    async fn on_guild_member_add(&self, ctx: Context, member: Member) {
        guild_member_addition::guild_member_addition(ctx, member, self.ids).await;
    }

    async fn on_guild_member_remove(
        &self,
        ctx: Context,
        guild_id: GuildId,
        user: User,
        member_data: Option<Member>,
    ) {
        guild_member_removal::guild_member_removal(ctx, guild_id, user, member_data, self.ids)
            .await;
    }

    fn on_message_create(&self, ctx: Context, message: Message) -> impl Future<Output = ()> + Send {
        message::message(ctx, &message);

        ready(())
    }

    async fn on_ready(&self, ctx: Context, ready: Ready) {
        ready::ready(ctx, ready, self.ids).await;
    }

    fn on_resumed(&self, ctx: Context, resume: Resumed) -> impl Future<Output = ()> + Send {
        resume::resume(ctx, resume);

        ready(())
    }

    async fn on_interaction_create(&self, ctx: Context, interaction: Interaction) {
        interaction_create::interaction_create(ctx, interaction).await;
    }
}

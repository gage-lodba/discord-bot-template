use discord::builder::embed::EmbedAuthorBuilder;
use discord::builder::embed::EmbedBuilder;
use discord::builder::message::MessageBuilder;
use discord::client::Context;
use discord::id::ChannelId;
use discord::id::GuildId;
use discord::model::guild::Member;
use discord::model::user::User;
use discord::timestamp::Timestamp;

use tracing::error;
use tracing::info;

use crate::config::Ids;
use crate::moderation;

pub(crate) async fn guild_member_removal(
    ctx: Context,
    guild_id: GuildId,
    user: User,
    _member_data_if_available: Option<Member>,
    ids: Ids,
) {
    // Discord fires this event for a voluntary leave, a kick *and* a ban, and the
    // payload does not say which. The audit entry that does say which is written
    // server-side and lands after this event, so hold the announcement briefly
    // and let the audit-log handler record the action first.
    //
    // Reference: `docs/gateway-events.md` (Guild Member Remove, Guild Audit Log
    // Entry Create).
    tokio::time::sleep(moderation::RECONCILE_DELAY).await;

    // A kick or ban already announced itself with its own embed, so posting
    // "Has left the server." as well would double up on the same removal.
    if let Some(action) = moderation::action_for(user.id) {
        info!(
            "{}[{}] left {} via {:?}; suppressing the leave announcement",
            user.display_name(),
            user.id,
            guild_id,
            action
        );
        return;
    }

    // `avatar_url()` falls back to Discord's default avatar when the user has no
    // custom avatar, which is the common case. `docs/reference.md`
    // (Image Formatting) documents the default-avatar URL.
    let avatar_url: String = user.avatar_url();

    let author = EmbedAuthorBuilder::new(user.display_name())
        .icon_url(avatar_url)
        .url(format!("https://discord.com/users/{}", user.id));

    let leave_embed = EmbedBuilder::new()
        .author(author)
        .color(0x00F4_4336)
        .description("Has left the server.")
        .timestamp(Timestamp::now());

    let builder = MessageBuilder::new().embed(leave_embed.build());
    let channel: ChannelId = ids.log_channel;

    if let Err(err) = channel.send_message(&ctx.http, &builder).await {
        error!(
            "failed to announce leave of {}[{}] from guild {}: {}",
            user.display_name(),
            user.id,
            guild_id,
            err
        );
        return;
    }

    info!("{} has left the server.", user.display_name());
}

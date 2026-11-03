use discord::builder::embed::EmbedAuthorBuilder;
use discord::builder::embed::EmbedBuilder;
use discord::builder::message::MessageBuilder;
use discord::client::Context;
use discord::id::ChannelId;
use discord::model::guild::Member;
use discord::timestamp::Timestamp;

use tracing::error;
use tracing::info;

use crate::config::Ids;

pub(crate) async fn guild_member_addition(ctx: Context, new_member: Member, ids: Ids) {
    // `avatar_url()` falls back to Discord's default avatar for the very common
    // case of a user without a custom avatar, so the join announcement is never
    // aborted before the welcome embed is sent. `docs/reference.md`
    // (Image Formatting) documents the default-avatar URL.
    let avatar_url: String = new_member.user.avatar_url();

    let author = EmbedAuthorBuilder::new(&new_member.user.username).icon_url(avatar_url);

    let embed = EmbedBuilder::new()
        .author(author)
        .color(0x004C_AF50)
        .description("Has joined the server.")
        .timestamp(Timestamp::now());

    let builder = MessageBuilder::new().embed(embed.build());
    let channel: ChannelId = ids.log_channel;

    if let Err(err) = channel.send_message(&ctx.http, &builder).await {
        error!(
            "failed to announce join of {}[{}] in guild {}: {}",
            new_member.user.username,
            new_member.user.id,
            new_member
                .guild_id
                .map_or_else(|| "unknown".to_owned(), |id| id.to_string()),
            err
        );
        return;
    }

    info!("{} has joined the server.", new_member.user.username);
}

use discord::client::Context;
use discord::model::message::Message;

use tracing::debug;

pub(crate) fn message(_ctx: Context, msg: &Message) {
    if msg.author.bot {
        return;
    }

    let name: &str = &msg.author.username;
    let user_id: String = msg.author.id.to_string();
    let user_msg: &str = &msg.content;

    // Message bodies are unbounded personal data, so they are logged at `debug!`
    // rather than `info!`. `main.rs` configures the subscriber with an INFO
    // filter, so this line is off unless the operator deliberately raises the
    // level: the default log must not record everything users type.
    //
    // Reading `Message::content` also depends on the privileged MESSAGE_CONTENT
    // gateway intent, which `main.rs` requests alongside GUILD_MESSAGES. Content
    // is only present when that intent is enabled for the application, so this
    // line is only ever as useful as the deployment's intent configuration.
    debug!("{}[{}] : {}", name, user_id, user_msg);
}

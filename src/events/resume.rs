use discord::client::Context;
use discord::model::ready::Resumed;

use tracing::info;

pub(crate) fn resume(_ctx: Context, _event: Resumed) {
    info!("Resumed connection");
}

//! The gateway (WebSocket) layer.
//!
//! This is the long-lived connection that keeps the bot online and delivers
//! events: connection lifecycle, heartbeat and resume, close-code handling, and
//! dispatch of received events to the cache and the event handler.
//!
//! Reference: `docs/gateway.md` (Connection Lifecycle, Sending Heartbeats,
//! Identifying, Resuming, Disconnecting), `docs/gateway-events.md` (Payload
//! Structure, Send/Receive Events) and `docs/opcodes-and-status-codes.md`
//! (Gateway Opcodes, Gateway Close Event Codes).

pub mod close;
pub mod dispatch;
pub mod payload;
pub(crate) mod shard;
pub mod shard_manager;

pub use close::CloseAction;
pub use dispatch::EventRoute;
pub use payload::{GatewayPayload, Opcode};
pub use shard_manager::ShardManager;

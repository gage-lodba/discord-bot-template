//! A from-scratch implementation of the [Discord API] for this repository's bot.
//!
//! Everything Discord-shaped in this crate — the gateway state machine, the REST
//! routes, the wire models, the builders and the local cache — is written
//! against the official Discord API reference. The `docs/<page>.md` citations in
//! the doc comments point at that reference; its local copy is not vendored in
//! this template. Nothing here wraps or copies an existing Discord library.
//!
//! # Layers
//!
//! * [`gateway`] — the WebSocket protocol: identify, heartbeat, resume and
//!   reconnect, dispatch of received events. Reference: `docs/gateway.md`,
//!   `docs/gateway-events.md`, `docs/opcodes-and-status-codes.md`.
//! * [`http`] — the REST client: authentication, rate-limit buckets, retries and
//!   the endpoints the bot uses. Reference: `docs/reference.md`,
//!   `docs/rate-limits.md` and the resource pages.
//! * [`model`] — the objects Discord sends and accepts. Reference: the resource
//!   pages in `docs/` (guild, channel, message, user, emoji, audit-log,
//!   components, interactions, ...).
//! * [`builder`] — payload builders for outbound requests (application commands,
//!   messages, embeds, interaction responses, presence).
//! * [`cache`] — the small in-memory state the bot needs: the guild, its
//!   channels/threads/roles and the current user.
//! * [`client`] — wiring for a bot: [`client::Client`], [`client::Context`] and
//!   the [`client::EventHandler`] trait the bot implements.
//!
//! # Scope
//!
//! The crate covers the v10 API surface the bot uses, not every resource Discord
//! documents. The modules are organised so a new endpoint is one function in
//! [`http`] plus a model and a builder, with no changes needed to the gateway.
//!
//! # Example
//!
//! ```no_run
//! use discord::client::{Client, EventHandler};
//! use discord::intents::GatewayIntents;
//!
//! struct Handler;
//!
//! impl EventHandler for Handler {}
//!
//! # async fn run(token: &str) -> Result<(), discord::Error> {
//! let mut client = Client::builder(token, GatewayIntents::GUILDS)
//!     .event_handler(Handler)
//!     .await?;
//! client.start().await
//! # }
//! ```
//!
//! [Discord API]: https://discord.com/developers/docs

#![forbid(unsafe_code)]

pub mod builder;
pub mod cache;
pub mod client;
pub mod error;
pub mod gateway;
pub mod http;
pub mod id;
pub mod intents;
pub mod model;
pub mod permissions;
pub mod prelude;
pub mod timestamp;

pub use client::Client;
pub use client::ClientBuilder;
pub use client::Context;
pub use client::Data;
pub use client::DataKey;
pub use client::EventHandler;
pub use error::Error;
pub use intents::GatewayIntents;

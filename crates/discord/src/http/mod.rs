//! The REST (HTTP) layer.
//!
//! A small client that authenticates with a bot token, applies Discord's
//! per-route and global rate limits, retries the cases the reference says are
//! retryable, and exposes one method per endpoint the bot uses.
//!
//! See `CONTRACT.md` for the exact API this module must expose,
//! `docs/reference.md` for authentication/base URL/User-Agent and
//! `docs/rate-limits.md` for the rate-limit contract.

pub mod client;
pub mod impls;
pub mod rate_limit;
pub mod routes;

//! Wire models for Discord resources.
//!
//! Each submodule models one resource page from the API reference: the fields
//! the bot consumes are typed, and fields it ignores are left out so that a new
//! Discord field cannot break deserialisation.

pub mod application;
pub mod audit_log;
pub mod channel;
pub mod component;
pub mod emoji;
pub mod guild;
pub mod message;
pub mod ready;
pub mod user;

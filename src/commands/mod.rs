//
//  Commands
//

// fun
// `/say` is filed here because its output is expressive rather than
// administrative, but it posts under the bot's own identity with no way to tell
// who really wrote it, so it is gated behind MANAGE_MESSAGES like the admin
// commands. See src/commands/say.rs.
pub(crate) mod say;

// Utility
pub(crate) mod info;
pub(crate) mod ping;

// Administration
pub(crate) mod ban;
pub(crate) mod kick;
pub(crate) mod role;

// Server Owner
pub(crate) mod dashboard;

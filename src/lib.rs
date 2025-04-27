pub mod commands;
pub mod data;
pub mod enforcement;
pub mod handlers;
pub mod llm;
pub mod logging;

// Customize these constants for your bot
pub const BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";
pub const CONSOLE_TARGET: &str = "dastardly_daemon";

pub use data::{Data, DataInner};
pub use data::{EnforcementAction, EnforcementState, PendingEnforcement};
// pub use data::{EnforcementHandler, EnforcementTask};
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

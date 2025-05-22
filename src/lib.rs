pub mod commands;
pub mod daemon_response;
pub mod data;
pub mod data_ext;
pub mod enforcement_new;
pub mod handlers;
pub mod logging;
pub mod status;

pub use data::{Data, DataInner};
pub use data::{EnforcementAction, EnforcementState, PendingEnforcement};
pub use data_ext::DataEnforcementExt;
// pub use data::{EnforcementHandler, EnforcementTask};
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

// Customize these constants for your bot
pub const BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";
pub const CONSOLE_TARGET: &str = "dastardly_daemon";

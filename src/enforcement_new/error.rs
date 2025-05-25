//! Error types for the enforcement system
//!
//! This module defines the various errors that can occur during enforcement operations.

use thiserror::Error;

/// Errors that can occur during enforcement operations
#[derive(Debug, Error)]
pub enum EnforcementError {
    /// Invalid state transition attempted
    #[error("Invalid state transition")]
    InvalidStateTransition,

    /// Enforcement record not found
    #[error("Enforcement not found: {0}")]
    NotFound(String),

    /// Discord API error
    #[error("Discord API error: {0}")]
    DiscordApi(#[from] Box<poise::serenity_prelude::Error>),

    /// Failed to get guild or member
    #[error("Failed to get guild or member: {0}")]
    GuildOrMemberNotFound(String),

    /// Action validation failed
    #[error("Action validation failed: {0}")]
    ValidationFailed(String),

    /// User not in voice channel
    #[error("User not in voice channel")]
    NotInVoiceChannel,

    // /// User not in the specified guild
    // #[error("User not in guild: {0}")]
    // UserNotInGuild(u64),
    /// No voice channels in guild
    #[error("No voice channels in guild: {0}")]
    NoVoiceChannels(u64),

    // /// Permission error
    // #[error("Permission error: {0}")]
    // PermissionDenied(String),

    // /// Error saving data
    // #[error("Error saving data: {0}")]
    // DataSaveError(String),
    /// Generic error
    #[error("Enforcement error: {0}")]
    Other(String),
}

impl From<poise::serenity_prelude::Error> for EnforcementError {
    fn from(error: poise::serenity_prelude::Error) -> Self {
        Self::DiscordApi(Box::new(error))
    }
}

// impl EnforcementError {

//     /// Create a validation error
//     pub fn validation(message: impl Into<String>) -> Self {
//         Self::ValidationFailed(message.into())
//     }

//     /// Create a guild or member not found error
//     pub fn guild_or_member(message: impl Into<String>) -> Self {
//         Self::GuildOrMemberNotFound(message.into())
//     }
// }

/// Convert a string into an EnforcementError
impl From<String> for EnforcementError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

/// Result type for enforcement operations
pub type EnforcementResult<T> = Result<T, EnforcementError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = EnforcementError::InvalidStateTransition;
        assert_eq!(error.to_string(), "Invalid state transition");

        let error = EnforcementError::NotFound("test-id".to_string());
        assert_eq!(error.to_string(), "Enforcement not found: test-id");

        let error = EnforcementError::from("Something went wrong".to_string());
        assert_eq!(error.to_string(), "Enforcement error: Something went wrong");

        // let error = EnforcementError::validation("Invalid parameters");
        // assert_eq!(error.to_string(), "Action validation failed: Invalid parameters");
    }
}

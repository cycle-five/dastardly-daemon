//! LLM integration for generating daemon responses
//!
//! This module provides functionality for generating text through an LLM API.
//! It is behind a feature flag "llm".

use crate::data::UserWarningState;

#[allow(unused)]
/// Configuration for the LLM client
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// API key for the LLM service
    pub api_key: String,
    /// Model to use for generation
    pub model: String,
    /// Temperature setting (0.0-1.0) where higher means more random
    pub temperature: f32,
    /// Maximum tokens to generate in a response
    pub max_tokens: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gpt-4".to_string(),
            temperature: 0.7,
            max_tokens: 100,
        }
    }
}

#[allow(unused)]
/// Types of responses that can be generated
#[derive(Debug, Clone, Copy)]
pub enum ResponseType {
    /// Response for warning a user
    Warning,
    /// Response for announcing a punishment
    Punishment,
    /// Response for the channel haunting punishment
    ChannelHaunt,
    /// Response for when a punishment is canceled
    Appeasement,
    /// Response for when the daemon is summoned
    Summoning,
    /// Response for when the daemon's chaos factor is changed
    ChaosRitual,
}

/// Generate a daemon-themed response based on the context and response type
///
/// # Arguments
///
/// * `context` - Context information about the situation
/// * `user_history` - Optional warning history for the user
/// * `response_type` - The type of response to generate
///
/// # Returns
///
/// A string containing the generated response
#[allow(clippy::unused_async)]
#[cfg(not(feature = "llm"))]
pub async fn generate_daemon_response(
    _warning_context: &str,
    user_history: Option<&UserWarningState>,
    response_type: ResponseType,
) -> String {
    // In a real implementation, this would call the LLM API
    // But for now we'll just return static responses

    // If we have user history and they have multiple warnings, reflect that in the response

    let repeat_offender = user_history
        .map(|state| state.warning_timestamps.len() > 2)
        .unwrap_or(false);

    match response_type {
        ResponseType::Warning => {
            if repeat_offender {
                "YOU AGAIN? *sigh* I was JUST getting comfortable in my realm of chaos! Fine... consider yourself warned, mortal. But my patience grows thin."
            } else {
                "I've been disturbed from my slumber to deal with... THIS? *dramatic eye roll* Consider yourself warned, mortal."
            }
        }
        ResponseType::Punishment => {
            if repeat_offender {
                "I've had ENOUGH of your antics! Time for you to feel my wrath... and trust me, I've been saving something special for repeat offenders."
            } else {
                "Your voice shall be cast into the void... for now. Perhaps this will teach you respect."
            }
        }
        ResponseType::ChannelHaunt => {
            "Time for a little game of musical chairs, mortal! Where will you end up? Even I don't know... and that's part of the fun! *cackles*"
        }
        ResponseType::Appeasement => {
            "The mods have offered a sacrifice on your behalf. I am... temporarily appeased. Consider yourself fortunate, mortal."
        }
        ResponseType::Summoning => {
            "WHO DARES TO SUMMON ME? *looks around* Oh, it's you lot again. What is it THIS time?"
        }
        ResponseType::ChaosRitual => {
            "I FEEL THE CHAOS FLOWING THROUGH ME! The ritual is complete. My powers grow... unpredictable."
        }
    }.to_string()
}

#[allow(dead_code)]
/// Non-feature-flagged version that returns static responses
#[cfg(feature = "llm")]
pub async fn generate_daemon_response(
    _context: &str,
    _user_history: Option<&UserWarningState>,
    _response_type: ResponseType,
) -> String {
    "RAWR IMPLEMENT THIS!".to_string()
}

//! Enforcement action types
//!
//! This module defines the different types of enforcement actions that can be applied
//! to users, with a more consistent parameter structure.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Type of enforcement action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnforcementActionType {
    /// No action (placeholder)
    None,
    /// Text channel timeout
    Mute,
    /// Server ban
    Ban,
    /// Server kick
    Kick,
    /// Voice mute
    VoiceMute,
    /// Voice deafen
    VoiceDeafen,
    /// Voice disconnect
    VoiceDisconnect,
    /// Voice channel haunting (teleportation)
    VoiceChannelHaunt,
}

impl fmt::Display for EnforcementActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Mute => write!(f, "Mute"),
            Self::Ban => write!(f, "Ban"),
            Self::Kick => write!(f, "Kick"),
            Self::VoiceMute => write!(f, "Voice Mute"),
            Self::VoiceDeafen => write!(f, "Voice Deafen"),
            Self::VoiceDisconnect => write!(f, "Voice Disconnect"),
            Self::VoiceChannelHaunt => write!(f, "Voice Channel Haunt"),
        }
    }
}

/// Parameters common to most enforcement actions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActionParams {
    /// Duration in seconds for timed actions, or delay for immediate actions
    pub duration: Option<u32>,

    /// Reason for the action (for audit logs)
    pub reason: Option<String>,
}

#[allow(unused)]
impl ActionParams {
    /// Create new action parameters with the specified duration
    pub fn new(duration: Option<u32>) -> Self {
        Self {
            duration,
            reason: None,
        }
    }

    /// Create new action parameters with the specified duration and reason
    pub fn with_reason(duration: Option<u32>, reason: impl Into<String>) -> Self {
        Self {
            duration,
            reason: Some(reason.into()),
        }
    }

    /// Get the duration or a default value
    pub fn duration_or_default(&self) -> u32 {
        self.duration.unwrap_or(0)
    }

    /// Check if the action has a duration (i.e., is timed)
    pub fn has_duration(&self) -> bool {
        self.duration.is_some() && self.duration.unwrap() > 0
    }
}

/// Parameters specific to the `VoiceChannelHaunt` action
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HauntParams {
    /// Number of times to teleport the user between channels
    pub teleport_count: Option<u32>,

    /// Seconds between each teleport
    pub interval: Option<u32>,

    /// Whether to eventually return the user to their original channel
    pub return_to_origin: Option<bool>,

    /// Original voice channel ID to potentially return to
    pub original_channel_id: Option<u64>,
}

impl HauntParams {
    /// Create new haunting parameters
    pub fn new(
        teleport_count: Option<u32>,
        interval: Option<u32>,
        return_to_origin: Option<bool>,
        original_channel_id: Option<u64>,
    ) -> Self {
        Self {
            teleport_count,
            interval,
            return_to_origin,
            original_channel_id,
        }
    }

    /// Get the teleport count or a default value
    pub fn teleport_count_or_default(&self) -> u32 {
        self.teleport_count.unwrap_or(3)
    }

    /// Get the interval or a default value
    pub fn interval_or_default(&self) -> u32 {
        self.interval.unwrap_or(10)
    }

    /// Get whether to return to origin or a default value
    pub fn return_to_origin_or_default(&self) -> bool {
        self.return_to_origin.unwrap_or(true)
    }
}

/// Enforcement actions that can be taken as part of a warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    /// No action (placeholder)
    None,

    /// Text channel timeout
    Mute(ActionParams),

    /// Server ban
    Ban(ActionParams),

    /// Server kick
    Kick(ActionParams),

    /// Voice mute
    VoiceMute(ActionParams),

    /// Voice deafen
    VoiceDeafen(ActionParams),

    /// Voice disconnect
    VoiceDisconnect(ActionParams),

    /// Voice channel haunting (teleportation)
    VoiceChannelHaunt(HauntParams),
}

impl Default for EnforcementAction {
    fn default() -> Self {
        Self::None
    }
}

impl EnforcementAction {
    /// Get the type of this action
    #[must_use]
    pub fn get_type(&self) -> EnforcementActionType {
        match self {
            Self::None => EnforcementActionType::None,
            Self::Mute(_) => EnforcementActionType::Mute,
            Self::Ban(_) => EnforcementActionType::Ban,
            Self::Kick(_) => EnforcementActionType::Kick,
            Self::VoiceMute(_) => EnforcementActionType::VoiceMute,
            Self::VoiceDeafen(_) => EnforcementActionType::VoiceDeafen,
            Self::VoiceDisconnect(_) => EnforcementActionType::VoiceDisconnect,
            Self::VoiceChannelHaunt(_) => EnforcementActionType::VoiceChannelHaunt,
        }
    }

    /// Check if this action needs reversal
    #[must_use]
    pub fn needs_reversal(&self) -> bool {
        match self {
            Self::Mute(params)
            | Self::Ban(params)
            | Self::VoiceMute(params)
            | Self::VoiceDeafen(params) => params.has_duration(),
            // These don't need reversal
            Self::Kick(_) | Self::VoiceDisconnect(_) | Self::VoiceChannelHaunt(_) | Self::None => {
                false
            }
        }
    }

    /// Check if this action is immediate (should be executed right away)
    #[must_use]
    pub fn is_immediate(&self) -> bool {
        match self {
            Self::Kick(params) | Self::VoiceDisconnect(params) => {
                // These are immediate if delay is 0 or not set
                !params.has_duration() || params.duration_or_default() == 0
            }
            Self::VoiceChannelHaunt(params) => {
                // Haunting is immediate if interval is 0 or not set
                params.interval.is_none() || params.interval.is_some_and(|v| v == 0)
            }
            // Nothing to delay and all other actions are always immediate.
            Self::Mute(_)
            | Self::Ban(_)
            | Self::VoiceMute(_)
            | Self::VoiceDeafen(_)
            | Self::None => true,
        }
    }

    /// Create a new Mute action
    pub fn mute(duration: impl Into<Option<u32>>) -> Self {
        Self::Mute(ActionParams::new(duration.into()))
    }

    /// Create a new `Ban` action
    pub fn ban(duration: impl Into<Option<u32>>) -> Self {
        Self::Ban(ActionParams::new(duration.into()))
    }

    /// Create a new `Kick` action
    pub fn kick(delay: impl Into<Option<u32>>) -> Self {
        Self::Kick(ActionParams::new(delay.into()))
    }

    /// Create a new `VoiceMute` action
    pub fn voice_mute(duration: impl Into<Option<u32>>) -> Self {
        Self::VoiceMute(ActionParams::new(duration.into()))
    }

    /// Create a new `VoiceDeafen` action
    pub fn voice_deafen(duration: impl Into<Option<u32>>) -> Self {
        Self::VoiceDeafen(ActionParams::new(duration.into()))
    }

    /// Create a new `VoiceDisconnect` action
    pub fn voice_disconnect(delay: impl Into<Option<u32>>) -> Self {
        Self::VoiceDisconnect(ActionParams::new(delay.into()))
    }

    /// Create a new `VoiceChannelHaunt` action
    pub fn voice_channel_haunt(
        teleport_count: impl Into<Option<u32>>,
        interval: impl Into<Option<u32>>,
        return_to_origin: impl Into<Option<bool>>,
        original_channel_id: impl Into<Option<u64>>,
    ) -> Self {
        Self::VoiceChannelHaunt(HauntParams::new(
            teleport_count.into(),
            interval.into(),
            return_to_origin.into(),
            original_channel_id.into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_needs_reversal() {
        // Actions without duration don't need reversal
        assert!(!EnforcementAction::mute(None).needs_reversal());
        assert!(!EnforcementAction::ban(None).needs_reversal());
        assert!(!EnforcementAction::voice_mute(None).needs_reversal());
        assert!(!EnforcementAction::voice_deafen(None).needs_reversal());

        // Actions with duration do need reversal
        assert!(EnforcementAction::mute(300).needs_reversal());
        assert!(EnforcementAction::ban(3600).needs_reversal());
        assert!(EnforcementAction::voice_mute(600).needs_reversal());
        assert!(EnforcementAction::voice_deafen(900).needs_reversal());

        // One-time actions never need reversal
        assert!(!EnforcementAction::kick(None).needs_reversal());
        assert!(!EnforcementAction::voice_disconnect(None).needs_reversal());
        assert!(!EnforcementAction::voice_channel_haunt(None, None, None, None).needs_reversal());
        assert!(!EnforcementAction::kick(10).needs_reversal());
        assert!(!EnforcementAction::voice_disconnect(5).needs_reversal());
        assert!(!EnforcementAction::voice_channel_haunt(3, 10, true, 12345).needs_reversal());
    }

    #[test]
    fn test_action_is_immediate() {
        // Actions without delay parameters are immediate
        assert!(EnforcementAction::mute(300).is_immediate());
        assert!(EnforcementAction::ban(3600).is_immediate());
        assert!(EnforcementAction::voice_mute(600).is_immediate());
        assert!(EnforcementAction::voice_deafen(900).is_immediate());

        // Actions with 0 delay are immediate
        assert!(EnforcementAction::kick(0).is_immediate());
        assert!(EnforcementAction::voice_disconnect(0).is_immediate());
        assert!(EnforcementAction::voice_channel_haunt(3, 0, true, 12345).is_immediate());

        // Actions with delay are not immediate
        assert!(!EnforcementAction::kick(10).is_immediate());
        assert!(!EnforcementAction::voice_disconnect(5).is_immediate());
        assert!(!EnforcementAction::voice_channel_haunt(3, 10, true, 12345).is_immediate());
    }

    #[test]
    fn test_action_type() {
        assert_eq!(
            EnforcementAction::None.get_type(),
            EnforcementActionType::None
        );
        assert_eq!(
            EnforcementAction::mute(300).get_type(),
            EnforcementActionType::Mute
        );
        assert_eq!(
            EnforcementAction::ban(3600).get_type(),
            EnforcementActionType::Ban
        );
        assert_eq!(
            EnforcementAction::kick(0).get_type(),
            EnforcementActionType::Kick
        );
        assert_eq!(
            EnforcementAction::voice_mute(600).get_type(),
            EnforcementActionType::VoiceMute
        );
        assert_eq!(
            EnforcementAction::voice_deafen(900).get_type(),
            EnforcementActionType::VoiceDeafen
        );
        assert_eq!(
            EnforcementAction::voice_disconnect(5).get_type(),
            EnforcementActionType::VoiceDisconnect
        );
        assert_eq!(
            EnforcementAction::voice_channel_haunt(3, 10, true, 12345).get_type(),
            EnforcementActionType::VoiceChannelHaunt
        );
    }

    #[test]
    fn test_action_params() {
        let params = ActionParams::new(Some(300));
        assert_eq!(params.duration, Some(300));
        assert!(params.has_duration());
        assert_eq!(params.duration_or_default(), 300);

        let params = ActionParams::new(None);
        assert_eq!(params.duration, None);
        assert!(!params.has_duration());
        assert_eq!(params.duration_or_default(), 0);

        let params = ActionParams::with_reason(Some(300), "Test reason");
        assert_eq!(params.duration, Some(300));
        assert_eq!(params.reason, Some("Test reason".to_string()));
    }

    #[test]
    fn test_haunt_params() {
        let params = HauntParams::new(Some(3), Some(10), Some(true), Some(12345));
        assert_eq!(params.teleport_count, Some(3));
        assert_eq!(params.interval, Some(10));
        assert_eq!(params.return_to_origin, Some(true));
        assert_eq!(params.original_channel_id, Some(12345));
        assert_eq!(params.teleport_count_or_default(), 3);
        assert_eq!(params.interval_or_default(), 10);
        assert!(params.return_to_origin_or_default());

        let params = HauntParams::default();
        assert_eq!(params.teleport_count, None);
        assert_eq!(params.interval, None);
        assert_eq!(params.return_to_origin, None);
        assert_eq!(params.original_channel_id, None);
        assert_eq!(params.teleport_count_or_default(), 3);
        assert_eq!(params.interval_or_default(), 10);
        assert!(params.return_to_origin_or_default());
    }
}

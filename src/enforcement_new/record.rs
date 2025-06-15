//! Enforcement record and state management
//!
//! This module defines the enforcement record structure and state machine for
//! managing enforcement lifecycle.

use crate::enforcement_new::{EnforcementAction, EnforcementError};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use super::EnforcementResult;

/// Enforcement action lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnforcementState {
    /// Not yet executed
    Pending,
    /// Applied but waiting for duration to expire
    Active,
    /// Action has been reversed (for timed actions)
    Reversed,
    /// Fully completed with no further action needed
    Completed,
    /// Manually cancelled by moderator
    Cancelled,
}

impl Default for EnforcementState {
    fn default() -> Self {
        Self::Pending
    }
}

impl std::fmt::Display for EnforcementState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Active => write!(f, "Active"),
            Self::Reversed => write!(f, "Reversed"),
            Self::Completed => write!(f, "Completed"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

/// Record of an enforcement action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforcementRecord {
    /// Unique ID of this enforcement
    pub id: String,
    /// ID of the warning that triggered this enforcement
    pub warning_id: String,
    /// ID of the user who is being enforced
    pub user_id: u64,
    /// ID of the guild where the enforcement is happening
    pub guild_id: u64,
    /// The action to be taken
    pub action: EnforcementAction,
    /// When to execute the action
    pub execute_at: DateTime<Utc>,
    /// When to automatically reverse the action (if applicable)
    pub reverse_at: Option<DateTime<Utc>>,
    /// Current state of the enforcement
    pub state: EnforcementState,
    /// When the record was created
    pub created_at: DateTime<Utc>,
    /// When the action was executed (if it has been)
    pub executed_at: Option<DateTime<Utc>>,
    /// When the action was reversed (if it has been)
    pub reversed_at: Option<DateTime<Utc>>,
    /// Whether the action has been executed (legacy field)
    pub executed: bool,
}

impl Default for EnforcementRecord {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            warning_id: String::new(),
            user_id: 0,
            guild_id: 0,
            action: EnforcementAction::default(),
            execute_at: Utc::now(),
            reverse_at: None,
            state: EnforcementState::Pending,
            created_at: Utc::now(),
            executed_at: None,
            reversed_at: None,
            executed: false,
        }
    }
}

impl EnforcementRecord {
    /// Create a new enforcement record
    pub fn new(
        warning_id: impl Into<String>,
        user_id: u64,
        guild_id: u64,
        action: EnforcementAction,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let execute_at = Self::calculate_execute_time(&action);

        Self {
            id,
            warning_id: warning_id.into(),
            user_id,
            guild_id,
            action,
            execute_at,
            state: EnforcementState::Pending,
            created_at: now,
            ..Default::default()
        }
    }

    /// Calculate the execution time for an action
    #[must_use]
    pub fn calculate_execute_time(action: &EnforcementAction) -> DateTime<Utc> {
        let now = Utc::now();

        match action {
            EnforcementAction::Kick(params) | EnforcementAction::VoiceDisconnect(params) => {
                if let Some(delay) = params.duration {
                    if delay > 0 {
                        return now + Duration::seconds(i64::from(delay));
                    }
                }
                now
            }
            EnforcementAction::VoiceChannelHaunt(params) => {
                if let Some(interval) = params.interval {
                    if interval > 0 {
                        return now + Duration::seconds(i64::from(interval));
                    }
                }
                now
            }
            // Other actions execute immediately
            _ => now,
        }
    }

    /// Calculate when an action should be reversed (if applicable)
    #[must_use]
    pub fn calculate_reversal_time(&self) -> Option<DateTime<Utc>> {
        if !self.action.needs_reversal() {
            return None;
        }

        let now = Utc::now();

        match &self.action {
            EnforcementAction::Mute(params)
            | EnforcementAction::Ban(params)
            | EnforcementAction::VoiceMute(params)
            | EnforcementAction::VoiceDeafen(params) => {
                if let Some(duration) = params.duration {
                    if duration > 0 {
                        return Some(now + Duration::seconds(i64::from(duration)));
                    }
                }
                None
            }
            // Other actions don't need reversal
            _ => None,
        }
    }

    /// Execute this enforcement, transitioning to Active or Completed
    ///
    /// # Errors
    /// Returns an error if the record is not in the Pending state
    pub fn execute(&mut self) -> EnforcementResult<()> {
        if self.state != EnforcementState::Pending {
            return Err(EnforcementError::InvalidStateTransition);
        }

        // Calculate reversal time if needed
        self.reverse_at = self.calculate_reversal_time();

        // Set state based on whether reversal is needed
        self.state = if self.reverse_at.is_some() {
            EnforcementState::Active
        } else {
            EnforcementState::Completed
        };

        self.executed_at = Some(Utc::now());
        self.executed = true; // For backward compatibility

        info!(
            enforcement_id = %self.id,
            user_id = %self.user_id,
            guild_id = %self.guild_id,
            action_type = %self.action.get_type(),
            reverse_at = ?self.reverse_at,
            "Enforcement action executed"
        );

        Ok(())
    }

    /// Reverse this enforcement, transitioning to Reversed
    ///
    /// # Errors
    /// Returns an error if the record is not in the Active state
    pub fn reverse(&mut self) -> EnforcementResult<()> {
        if self.state != EnforcementState::Active {
            return Err(EnforcementError::InvalidStateTransition);
        }

        self.state = EnforcementState::Reversed;
        self.reversed_at = Some(Utc::now());

        info!(
            enforcement_id = %self.id,
            user_id = %self.user_id,
            guild_id = %self.guild_id,
            action_type = %self.action.get_type(),
            "Enforcement action reversed"
        );

        Ok(())
    }

    /// Cancel this enforcement, transitioning to Cancelled
    ///
    /// # Errors
    /// Returns an error if the record is not in the Pending or Active state
    pub fn cancel(&mut self) -> EnforcementResult<()> {
        if self.state != EnforcementState::Pending && self.state != EnforcementState::Active {
            return Err(EnforcementError::InvalidStateTransition);
        }

        self.state = EnforcementState::Cancelled;

        info!(
            enforcement_id = %self.id,
            user_id = %self.user_id,
            guild_id = %self.guild_id,
            action_type = %self.action.get_type(),
            "Enforcement action cancelled"
        );

        Ok(())
    }

    /// Check if this enforcement is due for execution
    #[must_use]
    pub fn is_due_for_execution(&self) -> bool {
        self.state == EnforcementState::Pending && self.execute_at <= Utc::now()
    }

    /// Check if this enforcement is due for reversal
    #[must_use]
    pub fn is_due_for_reversal(&self) -> bool {
        self.state == EnforcementState::Active
            && self
                .reverse_at
                .is_some_and(|reverse_at| reverse_at <= Utc::now())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enforcement_state_transitions() {
        // Create a new record
        let mut record =
            EnforcementRecord::new("warning-123", 12345, 67890, EnforcementAction::mute(300));

        // Initial state should be Pending
        assert_eq!(record.state, EnforcementState::Pending);
        assert!(!record.executed);
        assert!(record.executed_at.is_none());

        // Execute should transition to Active (since it needs reversal)
        record.execute().unwrap();
        assert_eq!(record.state, EnforcementState::Active);
        assert!(record.executed);
        assert!(record.executed_at.is_some());
        assert!(record.reverse_at.is_some());

        // Reverse should transition to Reversed
        record.reverse().unwrap();
        assert_eq!(record.state, EnforcementState::Reversed);
        assert!(record.reversed_at.is_some());

        // Cannot reverse again
        assert!(record.reverse().is_err());

        // Test with an action that doesn't need reversal
        let mut record =
            EnforcementRecord::new("warning-123", 12345, 67890, EnforcementAction::kick(0));

        // Execute should transition directly to Completed
        record.execute().unwrap();
        assert_eq!(record.state, EnforcementState::Completed);
        assert!(record.executed);
        assert!(record.executed_at.is_some());
        assert!(record.reverse_at.is_none());

        // Cannot reverse a completed enforcement
        assert!(record.reverse().is_err());
    }

    #[test]
    fn test_cancellation() {
        // Test cancelling a pending enforcement
        let mut record =
            EnforcementRecord::new("warning-123", 12345, 67890, EnforcementAction::mute(300));

        record.cancel().unwrap();
        assert_eq!(record.state, EnforcementState::Cancelled);

        // Cannot execute a cancelled enforcement
        assert!(record.execute().is_err());

        // Test cancelling an active enforcement
        let mut record =
            EnforcementRecord::new("warning-123", 12345, 67890, EnforcementAction::mute(300));

        record.execute().unwrap();
        assert_eq!(record.state, EnforcementState::Active);

        record.cancel().unwrap();
        assert_eq!(record.state, EnforcementState::Cancelled);

        // Cannot reverse a cancelled enforcement
        assert!(record.reverse().is_err());
    }

    #[test]
    fn test_due_for_execution_or_reversal() {
        // Test a record that's due for execution
        let past = Utc::now() - Duration::seconds(10);
        let mut record =
            EnforcementRecord::new("warning-123", 12345, 67890, EnforcementAction::mute(300));
        record.execute_at = past;

        assert!(record.is_due_for_execution());
        assert!(!record.is_due_for_reversal());

        // Execute and test for reversal
        record.execute().unwrap();
        assert!(!record.is_due_for_execution());
        assert!(!record.is_due_for_reversal()); // Not due yet

        // Make it due for reversal
        record.reverse_at = Some(past);
        assert!(record.is_due_for_reversal());

        // Reverse and test neither should be true
        record.reverse().unwrap();
        assert!(!record.is_due_for_execution());
        assert!(!record.is_due_for_reversal());
    }

    #[test]
    fn test_calculate_execute_time() {
        let now = Utc::now();

        // Immediate actions
        let action = EnforcementAction::mute(300);
        let time = EnforcementRecord::calculate_execute_time(&action);
        assert!(time <= Utc::now());

        // Delayed actions
        let action = EnforcementAction::kick(60);
        let time = EnforcementRecord::calculate_execute_time(&action);
        assert!(time > now);
        let diff = time - now;
        assert!(diff.num_seconds() >= 59 && diff.num_seconds() <= 61);

        let action = EnforcementAction::voice_disconnect(30);
        let time = EnforcementRecord::calculate_execute_time(&action);
        assert!(time > now);
        let diff = time - now;
        assert!(diff.num_seconds() >= 29 && diff.num_seconds() <= 31);
    }
}

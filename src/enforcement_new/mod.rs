//! Enforcement system for Dastardly Daemon
//!
//! This module provides a refactored implementation of the enforcement system,
//! simplifying the state management and reducing code duplication.

mod action;
mod error;
mod handler;
mod record;
mod service;
mod store;

pub use action::{EnforcementAction, EnforcementActionType};
pub use error::{EnforcementError, EnforcementResult};
pub use handler::ActionHandlerRegistry;
pub use record::{EnforcementRecord, EnforcementState};
pub use service::EnforcementService;
pub use store::EnforcementStore;

/// Request type for the enforcement task
#[allow(unused)]
#[derive(Debug, Clone)]
pub enum EnforcementCheckRequest {
    /// Check for all pending enforcements regardless of timing
    CheckAll,
    /// Check for a specific user's enforcements in a specific guild
    CheckUser { user_id: u64, guild_id: u64 },
    /// Check for a specific enforcement by ID
    CheckEnforcement { enforcement_id: String },
    /// Shutdown the enforcement task
    Shutdown,
}

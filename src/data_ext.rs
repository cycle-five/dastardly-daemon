//! Data extension for enforcement system integration
//!
//! This module provides an extension to the existing Data struct for
//! compatibility with the new enforcement system.

use crate::data::{Data, EnforcementState, PendingEnforcement};
use crate::enforcement_new::{EnforcementAction, EnforcementRecord, EnforcementService};
use crate::enforcement_new::{EnforcementCheckRequest, EnforcementError};

use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use chrono::Utc;
use tracing::info;
use poise::serenity_prelude::Http;

/// Extension trait for Data
#[allow(async_fn_in_trait)]
pub trait DataEnforcementExt {
    /// Initialize the enforcement service
    fn init_enforcement_service(&mut self);
    
    /// Set the enforcement sender
    fn set_enforcement_service_sender(&mut self, tx: Sender<EnforcementCheckRequest>);
    
    /// Import old enforcements to the new system
    fn import_enforcements(&self);
    
    /// Export from new system to old data structures
    fn export_enforcements(&self);
    
    /// Create a new enforcement with the new system
    fn create_enforcement(
        &self,
        warning_id: impl Into<String>,
        user_id: u64,
        guild_id: u64,
        action: EnforcementAction,
    ) -> EnforcementRecord;
    
    /// Cancel all enforcements for a user in a guild
    async fn cancel_user_enforcements(
        &self,
        http: &Http,
        user_id: u64,
        guild_id: u64,
    ) -> Result<Vec<EnforcementRecord>, EnforcementError>;
    
    /// Process a specific enforcement
    async fn process_enforcement(
        &self,
        http: &Http,
        enforcement_id: &str,
    ) -> Result<(), EnforcementError>;
    
    /// Notify the enforcement task about a user
    async fn notify_enforcement_about_user(
        &self,
        user_id: u64,
        guild_id: u64,
    ) -> Result<(), EnforcementError>;
    
    /// Check if an enforcement record exists
    fn has_enforcement(&self, id: &str) -> bool;
    
    /// Get a pending enforcement by ID
    fn get_enforcement(
        &self,
        id: &str,
    ) -> Option<EnforcementRecord>;
    
    /// Convert old PendingEnforcement to new EnforcementRecord
    fn convert_to_new_enforcement(&self, old: &PendingEnforcement) -> EnforcementRecord;
    
    /// Convert new EnforcementRecord to old PendingEnforcement
    fn convert_to_old_enforcement(&self, new: &EnforcementRecord) -> PendingEnforcement;
    
    /// Clear pending enforcement from user warning state
    fn clear_pending_enforcement(&self, user_id: u64, guild_id: u64);
    
    /// Import and start the enforcement task
    fn import_and_start_enforcement(
        &mut self,
        http: Arc<Http>,
        check_interval_seconds: u64,
    );
}

impl DataEnforcementExt for Data {
    fn init_enforcement_service(&mut self) {
        // Create the enforcement service in the data
        let enforcement_service = EnforcementService::new();
        self.enforcement_service = Some(enforcement_service);
    }
    
    fn set_enforcement_service_sender(&mut self, tx: Sender<EnforcementCheckRequest>) {
        if let Some(ref mut service) = self.enforcement_service {
            service.set_sender(tx);
        }
    }
    
    fn import_enforcements(&self) {
        if let Some(ref mut service) = self.enforcement_service.clone() {
            service.store.import_from_old(self);
            info!("Imported enforcements to new enforcement system");
        }
    }
    
    fn export_enforcements(&self) {
        // We need to clone the service first to avoid borrowing self as both mutable and immutable
        let service_clone = self.enforcement_service.as_ref().map(|service| service.clone());
        
        if let Some(service) = service_clone {
            service.store.export_to_old(self);
            info!("Exported enforcements to old enforcement system");
        }
    }
    
    fn create_enforcement(
        &self,
        warning_id: impl Into<String>,
        user_id: u64,
        guild_id: u64,
        action: EnforcementAction,
    ) -> EnforcementRecord {
        if let Some(ref service) = self.enforcement_service {
            let record = service.create_enforcement(warning_id, user_id, guild_id, action);
            
            // For backward compatibility, also create in old system
            let old_record = self.convert_to_old_enforcement(&record);
            match old_record.state {
                EnforcementState::Pending => {
                    self.pending_enforcements.insert(record.id.clone(), old_record);
                }
                EnforcementState::Active => {
                    self.active_enforcements.insert(record.id.clone(), old_record);
                }
                _ => {
                    self.completed_enforcements.insert(record.id.clone(), old_record);
                }
            }
            
            record
        } else {
            panic!("Enforcement service must be initialized before creating enforcements");
        }
    }
    
    async fn cancel_user_enforcements(
        &self,
        http: &Http,
        user_id: u64,
        guild_id: u64,
    ) -> Result<Vec<EnforcementRecord>, EnforcementError> {
        if let Some(ref service) = self.enforcement_service {
            let result = service.cancel_all_for_user(http, user_id, guild_id).await?;
            
            // For backward compatibility, update old system
            self.export_enforcements();
            
            Ok(result)
        } else {
            Err(EnforcementError::Other("Enforcement service not initialized".to_string()))
        }
    }
    
    async fn process_enforcement(
        &self,
        http: &Http,
        enforcement_id: &str,
    ) -> Result<(), EnforcementError> {
        if let Some(ref service) = self.enforcement_service {
            let result = service.process_enforcement(http, enforcement_id).await;
            
            // For backward compatibility, update old system
            self.export_enforcements();
            
            result
        } else {
            Err(EnforcementError::Other("Enforcement service not initialized".to_string()))
        }
    }
    
    async fn notify_enforcement_about_user(
        &self,
        user_id: u64,
        guild_id: u64,
    ) -> Result<(), EnforcementError> {
        if let Some(ref service) = self.enforcement_service {
            service.notify_about_user(user_id, guild_id).await
        } else {
            Err(EnforcementError::Other("Enforcement service not initialized".to_string()))
        }
    }
    
    fn has_enforcement(&self, id: &str) -> bool {
        if let Some(ref service) = self.enforcement_service {
            service.store.get(id).is_some()
        } else {
            false
        }
    }
    
    fn get_enforcement(&self, id: &str) -> Option<EnforcementRecord> {
        if let Some(ref service) = self.enforcement_service {
            service.store.get(id).map(|e| e.clone())
        } else {
            None
        }
    }
    
    fn convert_to_new_enforcement(&self, old: &PendingEnforcement) -> EnforcementRecord {
        EnforcementRecord::from_old(old)
    }
    
    fn convert_to_old_enforcement(&self, new: &EnforcementRecord) -> PendingEnforcement {
        new.to_old()
    }
    
    fn clear_pending_enforcement(&self, user_id: u64, guild_id: u64) {
        let key = format!("{user_id}:{guild_id}");
        
        if let Some(mut state) = self.user_warning_states.get_mut(&key) {
            // Clear the pending enforcement
            if state.pending_enforcement.is_some() {
                info!("Clearing pending enforcement for user {user_id} in guild {guild_id}");
                let mut updated_state = state.value().clone();
                updated_state.pending_enforcement = None;
                updated_state.last_updated = Utc::now();
                
                // Update the state
                *state = updated_state;
            }
        }
    }
    
    fn import_and_start_enforcement(
        &mut self,
        http: Arc<Http>,
        check_interval_seconds: u64,
    ) {
        // Initialize if not already done
        if self.enforcement_service.is_none() {
            self.init_enforcement_service();
        }
        
        // We need to clone to avoid the mutable borrow issue
        let data_clone = self.clone();
        
        // We check if the service is initialized above so this is safe.
        if let Some(service) = self.enforcement_service.as_mut() {
            service.import_and_start(&data_clone, http, check_interval_seconds);
        }
    }
}
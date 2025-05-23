//! Enforcement store
//!
//! This module provides a centralized store for enforcement records.

use crate::enforcement_new::{EnforcementRecord, EnforcementState, EnforcementError, EnforcementResult};
use dashmap::DashMap;
use chrono::Utc;
use std::sync::Arc;

/// Store for enforcement records
#[derive(Clone)]
pub struct EnforcementStore {
    /// Single map containing all enforcements
    records: Arc<DashMap<String, EnforcementRecord>>,
}

impl Default for EnforcementStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EnforcementStore {
    /// Create a new enforcement store
    pub fn new() -> Self {
        Self {
            records: Arc::new(DashMap::new()),
        }
    }
    
    /// Add a new enforcement record
    pub fn add(&self, record: EnforcementRecord) {
        let id = record.id.clone();
        self.records.insert(id, record);
    }
    
    /// Get an enforcement record by ID
    pub fn get(&self, id: &str) -> Option<dashmap::mapref::one::Ref<'_, String, EnforcementRecord>> {
        self.records.get(id)
    }
    
    /// Get a mutable reference to an enforcement record by ID
    pub fn get_mut(&self, id: &str) -> Option<dashmap::mapref::one::RefMut<'_, String, EnforcementRecord>> {
        self.records.get_mut(id)
    }
    
    /// Remove an enforcement record by ID
    pub fn remove(&self, id: &str) -> Option<(String, EnforcementRecord)> {
        self.records.remove(id)
    }
    
    /// Get all enforcement records
    pub fn get_all(&self) -> Vec<EnforcementRecord> {
        self.records.iter().map(|e| e.value().clone()).collect()
    }
    
    /// Get pending enforcements due for execution
    pub fn get_pending_for_execution(&self) -> Vec<String> {
        let now = Utc::now();
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.state == EnforcementState::Pending && record.execute_at <= now {
                    Some(record.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get active enforcements due for reversal
    pub fn get_active_for_reversal(&self) -> Vec<String> {
        let now = Utc::now();
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.state == EnforcementState::Active 
                   && record.reverse_at.is_some() 
                   && record.reverse_at.unwrap() <= now {
                    Some(record.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get all enforcements for a user in a guild
    pub fn get_for_user(&self, user_id: u64, guild_id: u64) -> Vec<EnforcementRecord> {
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.user_id == user_id && record.guild_id == guild_id {
                    Some(record.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get pending enforcements for a user in a guild
    pub fn get_pending_for_user(&self, user_id: u64, guild_id: u64) -> Vec<EnforcementRecord> {
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.user_id == user_id && record.guild_id == guild_id && record.state == EnforcementState::Pending {
                    Some(record.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get active enforcements for a user in a guild
    pub fn get_active_for_user(&self, user_id: u64, guild_id: u64) -> Vec<EnforcementRecord> {
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.user_id == user_id && record.guild_id == guild_id && record.state == EnforcementState::Active {
                    Some(record.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Get all enforcements by state
    pub fn get_by_state(&self, state: EnforcementState) -> Vec<EnforcementRecord> {
        self.records
            .iter()
            .filter_map(|entry| {
                let record = entry.value();
                if record.state == state {
                    Some(record.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Execute an enforcement by ID
    pub fn execute_enforcement(&self, id: &str) -> EnforcementResult<EnforcementRecord> {
        if let Some(mut record) = self.get_mut(id) {
            if record.state != EnforcementState::Pending {
                return Err(EnforcementError::InvalidStateTransition);
            }
            
            record.execute()?;
            
            // Return a clone of the updated record
            let record_clone = record.clone();
            Ok(record_clone)
        } else {
            Err(EnforcementError::NotFound(id.to_string()))
        }
    }
    
    /// Reverse an enforcement by ID
    pub fn reverse_enforcement(&self, id: &str) -> EnforcementResult<EnforcementRecord> {
        if let Some(mut record) = self.get_mut(id) {
            if record.state != EnforcementState::Active {
                return Err(EnforcementError::InvalidStateTransition);
            }
            
            record.reverse()?;
            
            // Return a clone of the updated record
            let record_clone = record.clone();
            Ok(record_clone)
        } else {
            Err(EnforcementError::NotFound(id.to_string()))
        }
    }
    
    /// Cancel an enforcement by ID
    pub fn cancel_enforcement(&self, id: &str) -> EnforcementResult<EnforcementRecord> {
        if let Some(mut record) = self.get_mut(id) {
            if record.state != EnforcementState::Pending && record.state != EnforcementState::Active {
                return Err(EnforcementError::InvalidStateTransition);
            }
            
            record.cancel()?;
            
            // Return a clone of the updated record
            let record_clone = record.clone();
            Ok(record_clone)
        } else {
            Err(EnforcementError::NotFound(id.to_string()))
        }
    }
    
    /// Cancel all pending enforcements for a user in a guild
    pub fn cancel_all_for_user(&self, user_id: u64, guild_id: u64) -> Vec<EnforcementRecord> {
        let mut cancelled = Vec::new();
        
        for entry in self.records.iter() {
            let record = entry.value();
            if record.user_id == user_id && record.guild_id == guild_id && 
               (record.state == EnforcementState::Pending || record.state == EnforcementState::Active) {
                let id = record.id.clone();
                drop(entry); // Drop the immutable reference
                
                if let Ok(record) = self.cancel_enforcement(&id) {
                    cancelled.push(record);
                }
            }
        }
        
        cancelled
    }
    
//   /// Import records from the old system
//     pub fn import_from_old(&mut self, data: &crate::data::Data) {
//         // Import pending enforcements
//         for entry in &data.pending_enforcements {
//             let old_record = entry.value();
//             let new_record = EnforcementRecord::from_old(old_record);
//             self.add(new_record);
//         }
        
//         // Import active enforcements
//         for entry in &data.active_enforcements {
//             let old_record = entry.value();
//             let new_record = EnforcementRecord::from_old(old_record);
//             self.add(new_record);
//         }
        
//         // Import completed enforcements
//         for entry in &data.completed_enforcements {
//             let old_record = entry.value();
//             let new_record = EnforcementRecord::from_old(old_record);
//             self.add(new_record);
//         }
        
//         info!("Imported {} records from old enforcement system", self.records.len());
//     }
    
//     /// Export records to the old system (for backward compatibility)
//     pub fn export_to_old(&self, data: &crate::data::Data) {
//         // Clear old maps
//         data.pending_enforcements.clear();
//         data.active_enforcements.clear();
//         data.completed_enforcements.clear();
        
//         // Export records by state
//         for entry in self.records.iter() {
//             let record = entry.value();
//             let old_record = record.to_old();
            
//             match record.state {
//                 EnforcementState::Pending => {
//                     data.pending_enforcements.insert(record.id.clone(), old_record);
//                 }
//                 EnforcementState::Active => {
//                     data.active_enforcements.insert(record.id.clone(), old_record);
//                 }
//                 EnforcementState::Reversed | EnforcementState::Completed | EnforcementState::Cancelled => {
//                     data.completed_enforcements.insert(record.id.clone(), old_record);
//                 }
//             }
//         }
        
//         info!("Exported {} records to old enforcement system", self.records.len());
//     }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enforcement_new::EnforcementAction;
    
    #[test]
    fn test_add_and_get() {
        let store = EnforcementStore::new();
        let record = EnforcementRecord::new(
            "warning-123",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let id = record.id.clone();
        
        store.add(record);
        
        let retrieved = store.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().state, EnforcementState::Pending);
    }
    
    #[test]
    fn test_execute_and_reverse() {
        let store = EnforcementStore::new();
        let record = EnforcementRecord::new(
            "warning-123",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let id = record.id.clone();
        
        store.add(record);
        
        // Execute
        let result = store.execute_enforcement(&id);
        assert!(result.is_ok());
        let executed = result.unwrap();
        assert_eq!(executed.state, EnforcementState::Active);
        
        // Verify state in store
        let retrieved = store.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().state, EnforcementState::Active);
        
        // Reverse
        let result = store.reverse_enforcement(&id);
        assert!(result.is_ok());
        let reversed = result.unwrap();
        assert_eq!(reversed.state, EnforcementState::Reversed);
        
        // Verify state in store
        let retrieved = store.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().state, EnforcementState::Reversed);
    }
    
    #[test]
    fn test_cancel() {
        let store = EnforcementStore::new();
        let record = EnforcementRecord::new(
            "warning-123",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let id = record.id.clone();
        
        store.add(record);
        
        // Cancel
        let result = store.cancel_enforcement(&id);
        assert!(result.is_ok());
        let cancelled = result.unwrap();
        assert_eq!(cancelled.state, EnforcementState::Cancelled);
        
        // Verify state in store
        let retrieved = store.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().state, EnforcementState::Cancelled);
    }
    
    #[test]
    fn test_get_for_user() {
        let store = EnforcementStore::new();
        
        // Add multiple records for different users
        let record1 = EnforcementRecord::new(
            "warning-1",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let record2 = EnforcementRecord::new(
            "warning-2",
            12345,
            67890,
            EnforcementAction::voice_mute(300),
        );
        let record3 = EnforcementRecord::new(
            "warning-3",
            98765,
            67890,
            EnforcementAction::mute(300),
        );
        
        store.add(record1);
        store.add(record2);
        store.add(record3);
        
        // Test get_for_user
        let records = store.get_for_user(12345, 67890);
        assert_eq!(records.len(), 2);
        
        let records = store.get_for_user(98765, 67890);
        assert_eq!(records.len(), 1);
        
        let records = store.get_for_user(55555, 67890);
        assert_eq!(records.len(), 0);
    }
    
    #[test]
    fn test_get_by_state() {
        let store = EnforcementStore::new();
        
        // Add records in different states
        let record1 = EnforcementRecord::new(
            "warning-1",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let record2 = EnforcementRecord::new(
            "warning-2",
            12345,
            67890,
            EnforcementAction::voice_mute(300),
        );
        let record3 = EnforcementRecord::new(
            "warning-3",
            98765,
            67890,
            EnforcementAction::mute(300),
        );
        
        let id1 = record1.id.clone();
        let id2 = record2.id.clone();
        
        store.add(record1);
        store.add(record2);
        store.add(record3);
        
        // Execute one record
        let _ = store.execute_enforcement(&id1);
        
        // Cancel one record
        let _ = store.cancel_enforcement(&id2);
        
        // Test get_by_state
        let pending = store.get_by_state(EnforcementState::Pending);
        assert_eq!(pending.len(), 1);
        
        let active = store.get_by_state(EnforcementState::Active);
        assert_eq!(active.len(), 1);
        
        let cancelled = store.get_by_state(EnforcementState::Cancelled);
        assert_eq!(cancelled.len(), 1);
    }
    
    #[test]
    fn test_cancel_all_for_user() {
        let store = EnforcementStore::new();
        
        // Add multiple records for the same user
        let record1 = EnforcementRecord::new(
            "warning-1",
            12345,
            67890,
            EnforcementAction::mute(300),
        );
        let record2 = EnforcementRecord::new(
            "warning-2",
            12345,
            67890,
            EnforcementAction::voice_mute(300),
        );
        let record3 = EnforcementRecord::new(
            "warning-3",
            98765,
            67890,
            EnforcementAction::mute(300),
        );
        
        let id1 = record1.id.clone();
        
        store.add(record1);
        store.add(record2);
        store.add(record3);
        
        // Execute one record
        let _ = store.execute_enforcement(&id1);
        
        // Cancel all for user
        let cancelled = store.cancel_all_for_user(12345, 67890);
        assert_eq!(cancelled.len(), 2);
        
        // Verify states
        let all_for_user = store.get_for_user(12345, 67890);
        for record in all_for_user {
            assert_eq!(record.state, EnforcementState::Cancelled);
        }
        
        // Other user's record should be untouched
        let other_user = store.get_for_user(98765, 67890);
        assert_eq!(other_user.len(), 1);
        assert_eq!(other_user[0].state, EnforcementState::Pending);
    }
}
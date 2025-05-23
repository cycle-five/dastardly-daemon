//! Enforcement service
//!
//! This module provides a service for managing enforcement operations.

use crate::enforcement_new::{
    ActionHandlerRegistry, EnforcementAction, EnforcementCheckRequest, EnforcementError, 
    EnforcementRecord, EnforcementResult, EnforcementState, EnforcementStore
};
use poise::serenity_prelude::{GuildId, Http, UserId};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::Duration;
use tracing::{error, info};

/// Service for enforcement operations
#[derive(Clone)]
pub struct EnforcementService {
    /// Store for enforcement records
    pub store: EnforcementStore,
    /// Registry of action handlers
    handlers: Arc<ActionHandlerRegistry>,
    /// Sender for enforcement requests
    tx: Arc<Option<Sender<EnforcementCheckRequest>>>,
}

impl Default for EnforcementService {
    fn default() -> Self {
        Self::new()
    }
}

impl EnforcementService {
    /// Create a new enforcement service
    pub fn new() -> Self {
        Self {
            store: EnforcementStore::new(),
            handlers: Arc::new(ActionHandlerRegistry::new()),
            tx: Arc::new(None),
        }
    }
    
    /// Set the enforcement request sender
    pub fn set_sender(&mut self, tx: Sender<EnforcementCheckRequest>) {
        self.tx = Arc::new(Some(tx));
    }
    
    /// Create a new enforcement channel and return the sender
    pub fn create_enforcement_channel() -> Sender<EnforcementCheckRequest> {
        let (tx, rx) = mpsc::channel::<EnforcementCheckRequest>(100);
        let tx_clone = tx.clone();
        
        ENFORCEMENT_RECEIVER.with(|cell| {
            *cell.borrow_mut() = Some(rx);
        });
        
        tx_clone
    }
    
    /// Get the enforcement receiver if available
    pub fn take_enforcement_receiver() -> Option<Receiver<EnforcementCheckRequest>> {
        ENFORCEMENT_RECEIVER.with(|cell| cell.borrow_mut().take())
    }
    
    /// Start the enforcement task with a provided receiver
    pub fn start_task_with_receiver(
        self,
        http: Arc<Http>,
        rx: Receiver<EnforcementCheckRequest>,
        check_interval_seconds: u64,
    ) {
        // Spawn the task
        tokio::spawn(async move {
            self.enforcement_task(http, rx, check_interval_seconds).await;
        });
    }
    
    /// Create a new enforcement
    pub fn create_enforcement(
        &self,
        warning_id: impl Into<String>,
        user_id: u64,
        guild_id: u64,
        action: EnforcementAction,
    ) -> EnforcementRecord {
        let record = EnforcementRecord::new(warning_id, user_id, guild_id, action);
        self.store.add(record.clone());
        record
    }
    
    /// Process an enforcement - execute or reverse based on its current state
    pub async fn process_enforcement(
        &self,
        http: &Http,
        enforcement_id: &str,
    ) -> EnforcementResult<()> {
        if let Some(record) = self.store.get(enforcement_id) {
            let enforcement_id = record.id.clone();
            let user_id = record.user_id;
            let guild_id = record.guild_id;
            let state = record.state;
            let action = record.action.clone();
            
            drop(record); // Drop the immutable reference
            
            match state {
                EnforcementState::Pending => {
                    if let Ok(record) = self.store.execute_enforcement(&enforcement_id) {
                        // Execute the action
                        let guild_id = GuildId::new(guild_id);
                        let user_id = UserId::new(user_id);
                        
                        let result = self.handlers.execute(http, guild_id, user_id, &action).await;
                        
                        if let Err(e) = result {
                            error!("Failed to execute enforcement {enforcement_id}: {e}");
                            // Don't return the error, as we still want to keep the enforcement record in its new state
                        }
                    }
                }
                EnforcementState::Active => {
                    if let Some(reverse_at) = {
                        let record = self.store.get(&enforcement_id).unwrap();
                        record.reverse_at
                    } {
                        if reverse_at <= chrono::Utc::now() {
                            if let Ok(record) = self.store.reverse_enforcement(&enforcement_id) {
                                // Reverse the action
                                let guild_id = GuildId::new(guild_id);
                                let user_id = UserId::new(user_id);
                                
                                let result = self.handlers.reverse(http, guild_id, user_id, &action).await;
                                
                                if let Err(e) = result {
                                    error!("Failed to reverse enforcement {enforcement_id}: {e}");
                                    // Don't return the error, as we still want to keep the enforcement record in its new state
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            
            Ok(())
        } else {
            Err(EnforcementError::NotFound(enforcement_id.to_string()))
        }
    }
    
    /// Cancel an enforcement
    pub async fn cancel_enforcement(
        &self,
        http: &Http,
        enforcement_id: &str,
    ) -> EnforcementResult<()> {
        if let Some(record) = self.store.get(enforcement_id) {
            let state = record.state;
            
            // Only active enforcements need to be reversed when cancelled
            if state == EnforcementState::Active {
                let user_id = record.user_id;
                let guild_id = record.guild_id;
                let action = record.action.clone();
                
                drop(record); // Drop the immutable reference
                
                // Cancel in the store
                let _ = self.store.cancel_enforcement(enforcement_id)?;
                
                // Reverse the action
                let guild_id = GuildId::new(guild_id);
                let user_id = UserId::new(user_id);
                
                let result = self.handlers.reverse(http, guild_id, user_id, &action).await;
                
                if let Err(e) = result {
                    error!("Failed to reverse cancelled enforcement {enforcement_id}: {e}");
                    // Don't return the error, as we still want to keep the enforcement record in its cancelled state
                }
            } else {
                drop(record); // Drop the immutable reference
                
                // Just cancel in the store for pending enforcements
                let _ = self.store.cancel_enforcement(enforcement_id)?;
            }
            
            Ok(())
        } else {
            Err(EnforcementError::NotFound(enforcement_id.to_string()))
        }
    }
    
    /// Cancel all enforcements for a user in a guild
    pub async fn cancel_all_for_user(
        &self,
        http: &Http,
        user_id: u64,
        guild_id: u64,
    ) -> EnforcementResult<Vec<EnforcementRecord>> {
        let active_enforcements = self.store.get_active_for_user(user_id, guild_id);
        
        // Cancel all active enforcements first (these need reversal)
        for record in &active_enforcements {
            if let Err(e) = self.cancel_enforcement(http, &record.id).await {
                error!("Failed to cancel active enforcement {}: {}", record.id, e);
            }
        }
        
        // Cancel all pending enforcements (these don't need reversal)
        let cancelled = self.store.cancel_all_for_user(user_id, guild_id);
        
        Ok(cancelled)
    }
    
    /// Check all enforcements
    pub async fn check_all_enforcements(&self, http: &Http) -> EnforcementResult<()> {
        // Get all pending enforcements that need execution
        let pending_ids = self.store.get_pending_for_execution();
        
        // Get all active enforcements that need reversal
        let active_ids = self.store.get_active_for_reversal();
        
        // Execute pending enforcements
        for id in &pending_ids {
            if let Err(e) = self.process_enforcement(http, id).await {
                error!("Failed to process pending enforcement {id}: {e}");
            }
        }
        
        // Reverse active enforcements
        for id in &active_ids {
            if let Err(e) = self.process_enforcement(http, id).await {
                error!("Failed to process active enforcement {id} for reversal: {e}");
            }
        }
        
        Ok(())
    }
    
    /// Check enforcements for a specific user in a guild
    pub async fn check_user_enforcements(
        &self,
        http: &Http,
        user_id: u64,
        guild_id: u64,
    ) -> EnforcementResult<()> {
        // Get all pending enforcements for this user
        let pending = self.store.get_pending_for_user(user_id, guild_id);
        
        // Get all active enforcements for this user
        let active = self.store.get_active_for_user(user_id, guild_id);
        
        // Execute pending enforcements
        for record in &pending {
            if record.is_due_for_execution() {
                if let Err(e) = self.process_enforcement(http, &record.id).await {
                    error!("Failed to process pending enforcement {} for user {}: {}", record.id, user_id, e);
                }
            }
        }
        
        // Reverse active enforcements
        for record in &active {
            if record.is_due_for_reversal() {
                if let Err(e) = self.process_enforcement(http, &record.id).await {
                    error!("Failed to process active enforcement {} for user {}: {}", record.id, user_id, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Notify the enforcement task about a user
    pub async fn notify_about_user(&self, user_id: u64, guild_id: u64) -> EnforcementResult<()> {
        if let Some(tx) = &*self.tx {
            if let Err(e) = tx.send(EnforcementCheckRequest::CheckUser { user_id, guild_id }).await {
                error!("Failed to send user check request: {e}");
                return Err(EnforcementError::Other(format!("Failed to send user check request: {e}")));
            }
        } else {
            return Err(EnforcementError::Other("No enforcement task channel available".to_string()));
        }
        
        Ok(())
    }
    
    /// Notify the enforcement task about a specific enforcement
    pub async fn notify_about_enforcement(&self, enforcement_id: &str) -> EnforcementResult<()> {
        if let Some(tx) = &*self.tx {
            if let Err(e) = tx.send(EnforcementCheckRequest::CheckEnforcement { 
                enforcement_id: enforcement_id.to_string() 
            }).await {
                error!("Failed to send enforcement check request: {e}");
                return Err(EnforcementError::Other(format!("Failed to send enforcement check request: {e}")));
            }
        } else {
            return Err(EnforcementError::Other("No enforcement task channel available".to_string()));
        }
        
        Ok(())
    }
    
    /// Notify the enforcement task to check all enforcements
    pub async fn notify_check_all(&self) -> EnforcementResult<()> {
        if let Some(tx) = &*self.tx {
            if let Err(e) = tx.send(EnforcementCheckRequest::CheckAll).await {
                error!("Failed to send check all request: {e}");
                return Err(EnforcementError::Other(format!("Failed to send check all request: {e}")));
            }
        } else {
            return Err(EnforcementError::Other("No enforcement task channel available".to_string()));
        }
        
        Ok(())
    }
    
    /// The main enforcement task that periodically checks for enforcement actions
    async fn enforcement_task(
        &self,
        http: Arc<Http>,
        mut rx: Receiver<EnforcementCheckRequest>,
        check_interval_seconds: u64,
    ) {
        info!("Starting enforcement task with {check_interval_seconds}s interval");
        
        let check_interval = Duration::from_secs(check_interval_seconds);
        let mut interval = tokio::time::interval(check_interval);
        
        loop {
            tokio::select! {
                // Handle any incoming requests
                Some(request) = rx.recv() => {
                    match request {
                        EnforcementCheckRequest::CheckAll => {
                            info!("Received request to check all enforcements");
                            if let Err(e) = self.check_all_enforcements(&http).await {
                                error!("Error checking all enforcements: {e}");
                            }
                        },
                        EnforcementCheckRequest::CheckUser { user_id, guild_id } => {
                            info!("Received request to check enforcements for user {user_id} in guild {guild_id}");
                            if let Err(e) = self.check_user_enforcements(&http, user_id, guild_id).await {
                                error!("Error checking user enforcements: {e}");
                            }
                        },
                        EnforcementCheckRequest::CheckEnforcement { enforcement_id } => {
                            info!("Received request to check enforcement {enforcement_id}");
                            if let Err(e) = self.process_enforcement(&http, &enforcement_id).await {
                                error!("Error checking specific enforcement: {e}");
                            }
                        },
                        EnforcementCheckRequest::Shutdown => {
                            info!("Received shutdown request for enforcement task");
                            break;
                        }
                    }
                },
                
                // Periodic check
                _ = interval.tick() => {
                    info!("Performing periodic enforcement check");
                    if let Err(e) = self.check_all_enforcements(&http).await {
                        error!("Error in periodic enforcement check: {e}");
                    }
                }
            }
        }
        
        info!("Enforcement task shut down");
    }
    
    /// Import from old system and start the enforcement task
    pub fn import_and_start(
        &mut self,
        data: &crate::data::Data,
        http: Arc<Http>,
        check_interval_seconds: u64,
    ) {
        // // Import records from old system
        // self.store.import_from_old(data);
        
        // Create enforcement channel
        let tx = Self::create_enforcement_channel();
        self.set_sender(tx);
        
        // Start the enforcement task
        if let Some(rx) = Self::take_enforcement_receiver() {
            info!("Starting enforcement task...");
            self.clone().start_task_with_receiver(http, rx, check_interval_seconds);
        } else {
            error!("Failed to get enforcement receiver");
        }
    }
}

// Thread-local storage for the enforcement receiver
thread_local! {
    static ENFORCEMENT_RECEIVER: std::cell::RefCell<Option<Receiver<EnforcementCheckRequest>>> = 
        const { std::cell::RefCell::new(None) };
}
use crate::{Data, Error};
use crate::data::EnforcementAction;
use poise::serenity_prelude::{
    GuildId, 
    UserId,
    Http
};
use chrono::{Utc, DateTime};
use tracing::{info, error, warn};
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::time::Duration;
use std::sync::Arc;

/// Type of enforcement check request
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

/// Start the enforcement task and return a sender to communicate with it
pub fn start_enforcement_task(
    http: Arc<Http>,
    data: Data,
    check_interval_seconds: u64
) -> Sender<EnforcementCheckRequest> {
    // Create a channel for communication with the task
    let (tx, rx) = mpsc::channel::<EnforcementCheckRequest>(100);
    let tx_clone = tx.clone();
    
    // Spawn the task
    tokio::spawn(async move {
        enforcement_task(http, data, rx, check_interval_seconds).await;
    });
    
    tx_clone
}

/// The main enforcement task that periodically checks for enforcement actions
async fn enforcement_task(
    http: Arc<Http>,
    data: Data,
    mut rx: Receiver<EnforcementCheckRequest>,
    check_interval_seconds: u64
) {
    info!("Starting enforcement task with {}s interval", check_interval_seconds);
    
    let check_interval = Duration::from_secs(check_interval_seconds);
    let mut interval = tokio::time::interval(check_interval);
    
    loop {
        tokio::select! {
            // Handle any incoming requests
            Some(request) = rx.recv() => {
                match request {
                    EnforcementCheckRequest::CheckAll => {
                        info!("Received request to check all enforcements");
                        if let Err(e) = check_all_enforcements(&http, &data).await {
                            error!("Error checking all enforcements: {}", e);
                        }
                    },
                    EnforcementCheckRequest::CheckUser { user_id, guild_id } => {
                        info!("Received request to check enforcements for user {} in guild {}", user_id, guild_id);
                        if let Err(e) = check_user_enforcements(&http, &data, user_id, guild_id).await {
                            error!("Error checking user enforcements: {}", e);
                        }
                    },
                    EnforcementCheckRequest::CheckEnforcement { enforcement_id } => {
                        info!("Received request to check enforcement {}", enforcement_id);
                        if let Err(e) = check_specific_enforcement(&http, &data, &enforcement_id).await {
                            error!("Error checking specific enforcement: {}", e);
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
                if let Err(e) = check_all_enforcements(&http, &data).await {
                    error!("Error in periodic enforcement check: {}", e);
                }
            }
        }
    }
    
    info!("Enforcement task shut down");
}

/// Check all pending enforcements
async fn check_all_enforcements(http: &Http, data: &Data) -> Result<(), Error> {
    let now = Utc::now();
    let mut enforcements_to_execute = Vec::new();
    
    // Find enforcements that need to be executed
    for entry in &data.pending_enforcements {
        let pending = entry.value();
        if !pending.executed {
            let execute_at = DateTime::parse_from_rfc3339(&pending.execute_at)
                .map_or_else(|_| DateTime::<Utc>::MIN_UTC, |dt| dt.with_timezone(&Utc));
                
            if execute_at <= now {
                enforcements_to_execute.push(pending.id.clone());
            }
        }
    }
    
    // Execute each enforcement - use a clone to avoid borrowing issues
    let enforcements_to_execute_clone = enforcements_to_execute.clone();
    for id in enforcements_to_execute_clone {
        execute_enforcement(http, data, &id).await?;
    }
    
    // Save updated data
    if !enforcements_to_execute.is_empty() {
        if let Err(e) = data.save().await {
            error!("Failed to save data after executing enforcements: {}", e);
        }
    }
    
    Ok(())
}

/// Check enforcements for a specific user in a specific guild
async fn check_user_enforcements(http: &Http, data: &Data, user_id: u64, guild_id: u64) -> Result<(), Error> {
    let mut enforcements_to_execute = Vec::new();
    
    // Find enforcements for this user in this guild
    for entry in &data.pending_enforcements {
        let pending = entry.value();
        if !pending.executed 
            && pending.user_id == user_id 
            && pending.guild_id == guild_id {
            enforcements_to_execute.push(pending.id.clone());
        }
    }
    
    // Execute each enforcement
    let enforcements_to_execute_clone = enforcements_to_execute.clone();
    for id in enforcements_to_execute_clone {
        execute_enforcement(http, data, &id).await?;
    }
    
    // Save updated data
    if !enforcements_to_execute.is_empty() {
        if let Err(e) = data.save().await {
            error!("Failed to save data after executing user enforcements: {}", e);
        }
    }
    
    Ok(())
}

/// Check a specific enforcement
async fn check_specific_enforcement(http: &Http, data: &Data, enforcement_id: &str) -> Result<(), Error> {
    if let Some(pending) = data.pending_enforcements.get(enforcement_id) {
        if !pending.executed {
            let id = pending.id.clone();
            drop(pending); // Drop the borrow before calling execute_enforcement
            execute_enforcement(http, data, &id).await?;
            
            // Save data
            if let Err(e) = data.save().await {
                error!("Failed to save data after executing specific enforcement: {}", e);
            }
        }
    } else {
        warn!("Enforcement with ID {} not found", enforcement_id);
    }
    
    Ok(())
}

/// Execute a specific enforcement action
async fn execute_enforcement(http: &Http, data: &Data, enforcement_id: &str) -> Result<(), Error> {
    if let Some(mut pending) = data.pending_enforcements.get_mut(enforcement_id) {
        let guild_id = GuildId::new(pending.guild_id);
        let user_id = UserId::new(pending.user_id);
        
        // Execute the action based on the type
        match &pending.action {
            EnforcementAction::Mute { duration } => {
                match pending.executed {
                    false => {
                        // Apply mute (timeout)
                        info!("Muting user {} in guild {} for {} seconds", user_id, guild_id, duration);
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                let timeout_until = Utc::now() + chrono::Duration::seconds(*duration as i64);
                                if let Err(e) = member.disable_communication_until_datetime(http, timeout_until.into()).await {
                                    error!("Failed to mute user {}: {}", user_id, e);
                                } else {
                                    info!("Successfully muted user {} until {}", user_id, timeout_until);
                                }
                            }
                        }
                    },
                    true => {
                        // Remove the mute (automatic by Discord based on timestamp)
                        info!("Mute period expired for user {user_id} in guild {guild_id}");
                    }
                }
            },
            EnforcementAction::Ban { duration } => {
                match pending.executed {
                    false => {
                        // Ban the user
                        info!("Banning user {user_id} in guild {guild_id} for {duration} seconds");
                        
                        // Convert to days for unban scheduling (used later)
                        let reason = format!("Temporary ban from warning system for {duration} seconds");
                        
                        if let Err(e) = guild_id.ban_with_reason(http, user_id, 7, &reason).await {
                            error!("Failed to ban user {}: {}", user_id, e);
                        } else {
                            info!("Successfully banned user {}", user_id);
                            
                            // The task will auto-update this to executed = true, and we'll schedule the unban
                            // by creating a new pending enforcement
                        }
                    },
                    true => {
                        // Unban the user when duration expires
                        info!("Unbanning user {} in guild {}", user_id, guild_id);
                        if let Err(e) = guild_id.unban(http, user_id).await {
                            error!("Failed to unban user {}: {}", user_id, e);
                        } else {
                            info!("Successfully unbanned user {}", user_id);
                        }
                    }
                }
            },
            EnforcementAction::DelayedKick { delay } => {
                if *delay == 0 || pending.executed {
                    // Kick immediately or when the delay expires
                    info!("Kicking user {} from guild {}", user_id, guild_id);
                    if let Ok(guild) = guild_id.to_partial_guild(http).await {
                        if let Ok(member) = guild.member(http, user_id).await {
                            let reason = "Kicked by warning system";
                            if let Err(e) = member.kick_with_reason(http, reason).await {
                                error!("Failed to kick user {}: {}", user_id, e);
                            } else {
                                info!("Successfully kicked user {}", user_id);
                            }
                        }
                    }
                } else {
                    // This is a delayed kick that hasn't reached its time yet - do nothing
                    info!("Delayed kick for user {} is not ready yet", user_id);
                    // Will be handled when execution time is reached
                    return Ok(());
                }
            },
            EnforcementAction::None => {}
        }
        
        pending.executed = true;
        info!(
            target: crate::COMMAND_TARGET,
            enforcement_id = %enforcement_id,
            user_id = %pending.user_id,
            guild_id = %pending.guild_id,
            event = "enforcement_executed",
            "Enforcement action executed"
        );
    }
    
    Ok(())
}

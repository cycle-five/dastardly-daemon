use crate::data::EnforcementAction;
use crate::{Data, Error};
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{GuildId, Http, UserId};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::Duration;
use tracing::{error, info, warn};

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

/// Create a channel and return the sender
pub fn create_enforcement_channel() -> Sender<EnforcementCheckRequest> {
    let (tx, rx) = mpsc::channel::<EnforcementCheckRequest>(100);
    let tx_clone = tx.clone();

    // Store receiver in a static variable or return it
    ENFORCEMENT_RECEIVER.with(|cell| {
        *cell.borrow_mut() = Some(rx);
    });

    tx_clone
}

/// Start the enforcement task with a provided receiver
pub fn start_task_with_receiver(
    http: Arc<Http>,
    data: Data,
    rx: Receiver<EnforcementCheckRequest>,
    check_interval_seconds: u64,
) {
    // Spawn the task
    tokio::spawn(async move {
        enforcement_task(http, data, rx, check_interval_seconds).await;
    });
}

/// Start the enforcement task and return a sender to communicate with it
/// This is kept for backward compatibility
pub fn _start_enforcement_task(
    http: Arc<Http>,
    data: Data,
    check_interval_seconds: u64,
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

// Thread-local storage for the enforcement receiver
thread_local! {
    static ENFORCEMENT_RECEIVER: std::cell::RefCell<Option<Receiver<EnforcementCheckRequest>>> = const { std::cell::RefCell::new(None) };
}

/// Get the enforcement receiver if available
pub fn take_enforcement_receiver() -> Option<Receiver<EnforcementCheckRequest>> {
    ENFORCEMENT_RECEIVER.with(|cell| cell.borrow_mut().take())
}

/// The main enforcement task that periodically checks for enforcement actions
async fn enforcement_task(
    http: Arc<Http>,
    data: Data,
    mut rx: Receiver<EnforcementCheckRequest>,
    check_interval_seconds: u64,
) {
    info!(
        "Starting enforcement task with {}s interval",
        check_interval_seconds
    );

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
                            error!("Error checking all enforcements: {e}");
                        }
                    },
                    EnforcementCheckRequest::CheckUser { user_id, guild_id } => {
                        info!("Received request to check enforcements for user {} in guild {}", user_id, guild_id);
                        if let Err(e) = check_user_enforcements(&http, &data, user_id, guild_id).await {
                            error!("Error checking user enforcements: {e}");
                        }
                    },
                    EnforcementCheckRequest::CheckEnforcement { enforcement_id } => {
                        info!("Received request to check enforcement {}", enforcement_id);
                        if let Err(e) = check_specific_enforcement(&http, &data, &enforcement_id).await {
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
async fn check_user_enforcements(
    http: &Http,
    data: &Data,
    user_id: u64,
    guild_id: u64,
) -> Result<(), Error> {
    let mut enforcements_to_execute = Vec::new();

    info!("pending_enforcements: {:?}", data.pending_enforcements);
    // Find enforcements for this user in this guild
    for entry in &data.pending_enforcements {
        let pending = entry.value();
        if !pending.executed && pending.user_id == user_id && pending.guild_id == guild_id {
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
            error!(
                "Failed to save data after executing user enforcements: {}",
                e
            );
        }
    }

    Ok(())
}

/// Check a specific enforcement
async fn check_specific_enforcement(
    http: &Http,
    data: &Data,
    enforcement_id: &str,
) -> Result<(), Error> {
    if let Some(pending) = data.pending_enforcements.get(enforcement_id) {
        if !pending.executed {
            let id = pending.id.clone();
            drop(pending); // Drop the borrow before calling execute_enforcement
            execute_enforcement(http, data, &id).await?;

            // Save data
            if let Err(e) = data.save().await {
                error!(
                    "Failed to save data after executing specific enforcement: {}",
                    e
                );
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
                #[allow(clippy::match_bool)]
                match pending.executed {
                    false => {
                        // Apply mute (timeout)
                        info!("Muting user {user_id} in guild {guild_id} for {duration:?} seconds");
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                #[allow(clippy::cast_possible_wrap)]
                                let timeout_until = Utc::now()
                                    + chrono::Duration::seconds(duration.unwrap_or(0) as i64);
                                if let Err(e) = member
                                    .disable_communication_until_datetime(
                                        http,
                                        timeout_until.into(),
                                    )
                                    .await
                                {
                                    error!("Failed to mute user {user_id}: {e}");
                                } else {
                                    info!(
                                        "Successfully muted user {user_id} until {timeout_until}"
                                    );
                                }
                            }
                        }
                    }
                    true => {
                        // Remove the mute (automatic by Discord based on timestamp)
                        info!("Mute period expired for user {user_id} in guild {guild_id}");
                    }
                }
            }
            EnforcementAction::Ban { duration } => {
                #[allow(clippy::if_not_else)]
                if !pending.executed {
                    // Ban the user
                    info!("Banning user {user_id} in guild {guild_id} for {duration:?} seconds");

                    // Convert to days for unban scheduling (used later)
                    let reason =
                        format!("Temporary ban from warning system for {duration:?} seconds");

                    if let Err(e) = guild_id.ban_with_reason(http, user_id, 7, &reason).await {
                        error!("Failed to ban user {user_id}: {e}");
                    } else {
                        info!("Successfully banned user {user_id}");

                        // The task will auto-update this to executed = true, and we'll schedule the unban
                        // by creating a new pending enforcement
                    }
                } else {
                    // Unban the user when duration expires
                    info!("Unbanning user {user_id} in guild {guild_id}");
                    if let Err(e) = guild_id.unban(http, user_id).await {
                        error!("Failed to unban user {user_id}: {e}");
                    } else {
                        info!("Successfully unbanned user {user_id}");
                    }
                }
            }
            EnforcementAction::Kick { delay } => {
                if delay.is_none() || delay.is_some_and(|d| d == 0) || pending.executed {
                    // Kick immediately or when the delay expires
                    info!("Kicking user {user_id} from guild {guild_id}");
                    if let Ok(guild) = guild_id.to_partial_guild(http).await {
                        if let Ok(member) = guild.member(http, user_id).await {
                            let reason = "Kicked by warning system";
                            if let Err(e) = member.kick_with_reason(http, reason).await {
                                error!("Failed to kick user {user_id}: {e}");
                            } else {
                                info!("Successfully kicked user {user_id}");
                            }
                        }
                    }
                } else {
                    // This is a delayed kick that hasn't reached its time yet - do nothing
                    info!("Delayed kick for user {user_id} is not ready yet");
                    // Will be handled when execution time is reached
                    return Ok(());
                }
            }
            EnforcementAction::VoiceMute { duration } => {
                #[allow(clippy::match_bool)]
                match pending.executed {
                    false => {
                        // Apply voice mute
                        info!(
                            "Voice muting user {user_id} in guild {guild_id} for {duration:?} seconds"
                        );
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                use poise::serenity_prelude::builder::EditMember;

                                // Apply voice mute
                                if let Err(e) =
                                    member.edit(http, EditMember::new().mute(true)).await
                                {
                                    error!("Failed to voice mute user {}: {}", user_id, e);
                                } else {
                                    info!("Successfully voice muted user {}", user_id);

                                    // If there's a duration, schedule an un-mute task
                                    if let Some(dur) = duration {
                                        if *dur > 0 {
                                            // Mute is active, schedule an unmute task
                                            // This could be implemented by creating a new enforcement
                                            // with the executed flag set to true that will unmute when processed
                                            // But for now we'll rely on manual unmuting
                                            info!(
                                                "Voice mute will need to be manually removed after {} seconds",
                                                dur
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    true => {
                        // Remove the voice mute
                        info!("Voice mute period expired for user {user_id} in guild {guild_id}");
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                use poise::serenity_prelude::builder::EditMember;

                                if let Err(e) =
                                    member.edit(http, EditMember::new().mute(false)).await
                                {
                                    error!(
                                        "Failed to remove voice mute from user {}: {}",
                                        user_id, e
                                    );
                                } else {
                                    info!("Successfully removed voice mute from user {}", user_id);
                                }
                            }
                        }
                    }
                }
            }
            EnforcementAction::VoiceDeafen { duration } => {
                #[allow(clippy::match_bool)]
                match pending.executed {
                    false => {
                        // Apply voice deafen
                        info!(
                            "Voice deafening user {user_id} in guild {guild_id} for {duration:?} seconds"
                        );
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                use poise::serenity_prelude::builder::EditMember;

                                // Apply voice deafen
                                if let Err(e) =
                                    member.edit(http, EditMember::new().deafen(true)).await
                                {
                                    error!("Failed to voice deafen user {}: {}", user_id, e);
                                } else {
                                    info!("Successfully voice deafened user {}", user_id);

                                    // If there's a duration, schedule an un-deafen task
                                    if let Some(dur) = duration {
                                        if *dur > 0 {
                                            // Deafen is active, scheduling an undeafen would require a separate task
                                            info!(
                                                "Voice deafen will need to be manually removed after {} seconds",
                                                dur
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    true => {
                        // Remove the voice deafen
                        info!("Voice deafen period expired for user {user_id} in guild {guild_id}");
                        if let Ok(guild) = guild_id.to_partial_guild(http).await {
                            if let Ok(mut member) = guild.member(http, user_id).await {
                                use poise::serenity_prelude::builder::EditMember;

                                if let Err(e) =
                                    member.edit(http, EditMember::new().deafen(false)).await
                                {
                                    error!(
                                        "Failed to remove voice deafen from user {}: {}",
                                        user_id, e
                                    );
                                } else {
                                    info!(
                                        "Successfully removed voice deafen from user {}",
                                        user_id
                                    );
                                }
                            }
                        }
                    }
                }
            }
            EnforcementAction::VoiceDisconnect { delay } => {
                if delay.is_none() || delay.is_some_and(|d| d == 0) || pending.executed {
                    // Disconnect immediately or when the delay expires
                    info!("Disconnecting user {user_id} from voice in guild {guild_id}");
                    if let Ok(guild) = guild_id.to_partial_guild(http).await {
                        if let Ok(member) = guild.member(http, user_id).await {
                            // Disconnect from voice channel
                            if let Err(e) = member.disconnect_from_voice(http).await {
                                error!("Failed to disconnect user {} from voice: {}", user_id, e);
                            } else {
                                info!("Successfully disconnected user {} from voice", user_id);
                            }
                        }
                    }
                } else {
                    // This is a delayed disconnect that hasn't reached its time yet - do nothing
                    info!("Delayed voice disconnect for user {user_id} is not ready yet");
                    // Will be handled when execution time is reached
                    return Ok(());
                }
            }
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

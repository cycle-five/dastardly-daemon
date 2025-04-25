use crate::data::EnforcementAction;
use crate::{Data, Error};
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{GuildId, Http, UserId};
use serenity::all::{CacheHttp, ChannelId};
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

/// Helper function to get guild and member information
async fn get_guild_and_member(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
) -> Result<(serenity::all::PartialGuild, serenity::all::Member), Error> {
    let guild = guild_id
        .to_partial_guild(http)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    let member = guild
        .member(http, user_id)
        .await
        .map_err(|e| Error::from(e.to_string()))?;

    Ok((guild, member))
}

/// Handle mute enforcement action
async fn handle_mute_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    duration: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    if is_executed {
        // Remove the mute (automatic by Discord based on timestamp)
        info!("Mute period expired for user {user_id} in guild {guild_id}");
        return Ok(());
    }

    // Apply mute (timeout)
    info!("Muting user {user_id} in guild {guild_id} for {duration:?} seconds");

    if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
        #[allow(clippy::cast_possible_wrap)]
        let timeout_until = Utc::now() + chrono::Duration::seconds(duration.unwrap_or(0) as i64);

        match member
            .disable_communication_until_datetime(http, timeout_until.into())
            .await
        {
            Ok(()) => {
                info!("Successfully muted user {user_id} until {timeout_until}");
            }
            Err(e) => {
                error!("Failed to mute user {user_id}: {e}");
            }
        }
    }

    Ok(())
}

/// Handle ban enforcement action
async fn handle_ban_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    duration: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    if !is_executed {
        // Ban the user
        info!("Banning user {user_id} in guild {guild_id} for {duration:?} seconds");

        let reason = format!("Temporary ban from warning system for {duration:?} seconds");

        match guild_id.ban_with_reason(http, user_id, 7, &reason).await {
            Ok(()) => info!("Successfully banned user {user_id}"),
            Err(e) => error!("Failed to ban user {user_id}: {e}"),
        }
    } else {
        // Unban the user when duration expires
        info!("Unbanning user {user_id} in guild {guild_id}");
        match guild_id.unban(http, user_id).await {
            Ok(()) => info!("Successfully unbanned user {user_id}"),
            Err(e) => error!("Failed to unban user {user_id}: {e}"),
        }
    }

    Ok(())
}

/// Handle kick enforcement action
async fn handle_kick_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    delay: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    if delay.is_none() || delay.is_some_and(|d| d == 0) || is_executed {
        // Kick immediately or when the delay expires
        info!("Kicking user {user_id} from guild {guild_id}");

        if let Ok((_, member)) = get_guild_and_member(http, guild_id, user_id).await {
            let reason = "Kicked by warning system";
            match member.kick_with_reason(http, reason).await {
                Ok(()) => info!("Successfully kicked user {user_id}"),
                Err(e) => error!("Failed to kick user {user_id}: {e}"),
            }
        }
    } else {
        // This is a delayed kick that hasn't reached its time yet - do nothing
        info!("Delayed kick for user {user_id} is not ready yet");
        return Ok(());
    }

    Ok(())
}

/// Handle voice mute enforcement action
async fn handle_voice_mute_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    duration: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    use poise::serenity_prelude::builder::EditMember;

    if !is_executed {
        // Apply voice mute
        info!("Voice muting user {user_id} in guild {guild_id} for {duration:?} seconds");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            match member.edit(http, EditMember::new().mute(true)).await {
                Ok(()) => {
                    info!("Successfully voice muted user {user_id}");

                    // If there's a duration, log that it will need manual removal
                    if let Some(dur) = duration {
                        if *dur > 0 {
                            info!(
                                "Voice mute will need to be manually removed after {dur} seconds"
                            );
                        }
                    }
                }
                Err(e) => error!("Failed to voice mute user {user_id}: {e}"),
            }
        }
    } else {
        // Remove the voice mute
        info!("Voice mute period expired for user {user_id} in guild {guild_id}");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            match member.edit(http, EditMember::new().mute(false)).await {
                Ok(()) => info!("Successfully removed voice mute from user {user_id}"),
                Err(e) => error!("Failed to remove voice mute from user {user_id}: {e}"),
            }
        }
    }

    Ok(())
}

/// Handle voice deafen enforcement action
async fn handle_voice_deafen_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    duration: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    use poise::serenity_prelude::builder::EditMember;

    if !is_executed {
        // Apply voice deafen
        info!("Voice deafening user {user_id} in guild {guild_id} for {duration:?} seconds");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            match member.edit(http, EditMember::new().deafen(true)).await {
                Ok(()) => {
                    info!("Successfully voice deafened user {user_id}");

                    // If there's a duration, log that it will need manual removal
                    if let Some(dur) = duration {
                        if *dur > 0 {
                            info!(
                                "Voice deafen will need to be manually removed after {dur} seconds"
                            );
                        }
                    }
                }
                Err(e) => error!("Failed to voice deafen user {user_id}: {e}"),
            }
        }
    } else {
        // Remove the voice deafen
        info!("Voice deafen period expired for user {user_id} in guild {guild_id}");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            match member.edit(http, EditMember::new().deafen(false)).await {
                Ok(()) => info!("Successfully removed voice deafen from user {user_id}"),
                Err(e) => error!("Failed to remove voice deafen from user {user_id}: {e}"),
            }
        }
    }

    Ok(())
}

/// Handle voice disconnect enforcement action
async fn handle_voice_disconnect_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    delay: Option<&u64>,
    is_executed: bool,
) -> Result<(), Error> {
    if delay.is_none() || delay.is_some_and(|d| *d == 0) || is_executed {
        // Disconnect immediately or when the delay expires
        info!("Disconnecting user {user_id} from voice in guild {guild_id}");

        if let Ok((_, member)) = get_guild_and_member(http, guild_id, user_id).await {
            // Disconnect from voice channel
            match member.disconnect_from_voice(http).await {
                Ok(_) => info!("Successfully disconnected user {user_id} from voice"),
                Err(e) => error!("Failed to disconnect user {user_id} from voice: {e}"),
            }
        }
    } else {
        // This is a delayed disconnect that hasn't reached its time yet - do nothing
        info!("Delayed voice disconnect for user {user_id} is not ready yet");
        return Ok(());
    }

    Ok(())
}

/// Get the current voice channel for a user
async fn get_user_voice_channel(
    cache_http: &impl CacheHttp,
    guild_id: GuildId,
    user_id: UserId,
) -> Option<ChannelId> {
    let guild = cache_http.cache().map(|g| g.guild(guild_id)).flatten();
    let guild = match guild {
        Some(g) => g,
        None => {
            error!("Guild {guild_id} not found in cache");
            return None;
        }
    };

    guild
        .voice_states
        .get(&user_id)
        .and_then(|voice_state| voice_state.channel_id)
}

/// Get all voice channels in a guild
async fn get_guild_voice_channels(
    http: &Http,
    guild: &serenity::all::PartialGuild,
) -> Result<Vec<ChannelId>, Error> {
    let channels = guild.channels(http).await?;

    let voice_channels = channels
        .iter()
        .filter_map(|(id, channel)| {
            if channel.kind == serenity::all::ChannelType::Voice {
                Some(*id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    Ok(voice_channels)
}

/// Handle voice channel haunting enforcement action
async fn handle_voice_channel_haunt_action(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    teleport_count: &Option<u64>,
    interval: &Option<u64>,
    return_to_origin: &Option<bool>,
    original_channel_id: &Option<u64>,
    is_executed: bool,
) -> Result<(), Error> {
    // If already executed, nothing to do
    if is_executed {
        return Ok(());
    }

    info!("Beginning voice channel haunting for user {user_id} in guild {guild_id}");

    // Get guild
    let (guild, _) = match get_guild_and_member(http, guild_id, user_id).await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to get guild for haunting: {e}");
            return Ok(());
        }
    };

    // Find the user's current voice channel
    let current_voice_channel = get_user_voice_channel(http, guild_id, user_id).await;

    let voice_channel_id = if let Some(id) = current_voice_channel {
        id
    } else {
        error!("User {user_id} is not in a voice channel, cannot haunt");
        return Ok(());
    };

    // Store the original channel ID if it's not already set
    let original_id = original_channel_id.unwrap_or(voice_channel_id.get());

    // Get all voice channels in the guild
    let voice_channels = match get_guild_voice_channels(http, &guild).await {
        Ok(channels) => channels,
        Err(e) => {
            error!("Failed to get voice channels: {e}");
            return Ok(());
        }
    };

    if voice_channels.is_empty() {
        error!("No voice channels found in guild {guild_id} for haunting");
        return Ok(());
    }

    // Default parameters or use provided ones
    let (teleport_count, delay_count) = {
        let mut rng = rand::thread_rng();
        let count = teleport_count.unwrap_or_else(|| rand::Rng::gen_range(&mut rng, 1..=3));
        let delay = interval.unwrap_or_else(|| rand::Rng::gen_range(&mut rng, 5..=15));
        (count, delay)
    };
    let return_to_original = return_to_origin.unwrap_or(true);

    // Schedule the haunting for later execution
    let http_arc = Arc::new(http);
    let guild_id_copy = guild_id;
    let user_id_copy = user_id;
    let voice_channels_copy = voice_channels.clone();

    //tokio::spawn(async move {
    let mut failed = false;

    for i in 0..teleport_count {
        if failed {
            break;
        }

        // Pick a random channel (different from current on first teleport)
        let random_channel =
            select_random_voice_channel(&voice_channels_copy, i == 0, voice_channel_id);

        // Move the user to the random channel
        failed = !teleport_user(
            &http_arc.clone(),
            guild_id_copy,
            user_id_copy,
            random_channel,
        )
        .await;

        // Wait before the next teleport if we haven't failed
        if !failed {
            tokio::time::sleep(tokio::time::Duration::from_secs(delay_count)).await;
        }
    }

    // Return the user to their original channel if specified and we haven't failed
    if return_to_original && !failed {
        let original_channel = ChannelId::new(original_id);
        teleport_user(&http_arc, guild_id_copy, user_id_copy, original_channel).await;
    }
    //});

    info!("Voice channel haunting scheduled for user {user_id}");
    Ok(())
}

/// Select a random voice channel, optionally ensuring it's different from the current one
fn select_random_voice_channel(
    voice_channels: &[ChannelId],
    must_be_different: bool,
    current_channel: ChannelId,
) -> ChannelId {
    let rng = &mut rand::thread_rng();
    if !must_be_different || voice_channels.len() <= 1 {
        // If we don't need a different channel or there's only one channel, just pick randomly
        let idx = rand::Rng::gen_range(rng, 0..voice_channels.len());
        return voice_channels[idx];
    }

    // We need a different channel and have multiple options
    loop {
        let idx = rand::Rng::gen_range(rng, 0..voice_channels.len());
        let channel = voice_channels[idx];

        if channel != current_channel {
            return channel;
        }
    }
}

/// Teleport a user to a specific voice channel
async fn teleport_user(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    channel_id: ChannelId,
) -> bool {
    match guild_id.to_partial_guild(http).await {
        Ok(guild) => match guild.member(http, user_id).await {
            Ok(mut member) => {
                info!("Teleporting user {user_id} to channel {channel_id}");
                match member
                    .edit(
                        http,
                        serenity::builder::EditMember::new().voice_channel(channel_id),
                    )
                    .await
                {
                    Ok(()) => true,
                    Err(e) => {
                        error!("Failed to teleport user {user_id}: {e}");
                        false
                    }
                }
            }
            Err(e) => {
                error!("Failed to get member {user_id}: {e}");
                false
            }
        },
        Err(e) => {
            error!("Failed to get guild {guild_id}: {e}");
            false
        }
    }
}

/// Execute a specific enforcement action
async fn execute_enforcement(http: &Http, data: &Data, enforcement_id: &str) -> Result<(), Error> {
    if let Some(mut pending) = data.pending_enforcements.get_mut(enforcement_id) {
        let guild_id = GuildId::new(pending.guild_id);
        let user_id = UserId::new(pending.user_id);
        let is_executed = pending.executed;

        // Execute the action based on the type
        match &pending.action {
            EnforcementAction::Mute { duration } => {
                handle_mute_action(http, guild_id, user_id, duration, is_executed).await?;
            }
            EnforcementAction::VoiceChannelHaunt {
                teleport_count,
                interval,
                return_to_origin,
                original_channel_id,
            } => {
                handle_voice_channel_haunt_action(
                    http,
                    guild_id,
                    user_id,
                    teleport_count,
                    interval,
                    return_to_origin,
                    original_channel_id,
                    is_executed,
                )
                .await?;
            }
            EnforcementAction::Ban { duration } => {
                handle_ban_action(http, guild_id, user_id, duration, is_executed).await?;
            }
            EnforcementAction::Kick { delay } => {
                handle_kick_action(http, guild_id, user_id, delay, is_executed).await?;
            }
            EnforcementAction::VoiceMute { duration } => {
                handle_voice_mute_action(http, guild_id, user_id, duration, is_executed).await?;
            }
            EnforcementAction::VoiceDeafen { duration } => {
                handle_voice_deafen_action(http, guild_id, user_id, duration, is_executed).await?;
            }
            EnforcementAction::VoiceDisconnect { delay } => {
                handle_voice_disconnect_action(
                    http,
                    guild_id,
                    user_id,
                    delay.as_ref(),
                    is_executed,
                )
                .await?;
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

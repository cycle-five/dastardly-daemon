use crate::data::{EnforcementAction, EnforcementState};
use crate::{Data, Error};
use chrono::{DateTime, Utc};
use poise::serenity_prelude::{GuildId, Http, UserId, builder::EditMember};
use serenity::all::{CacheHttp, ChannelId};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::Duration;
use tracing::{error, info, warn};

/// Type of enforcement check request
#[allow(dead_code)]
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
#[must_use]
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

// Thread-local storage for the enforcement receiver
thread_local! {
    static ENFORCEMENT_RECEIVER: std::cell::RefCell<Option<Receiver<EnforcementCheckRequest>>> = const { std::cell::RefCell::new(None) };
}

/// Get the enforcement receiver if available
#[must_use]
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

/// Check all enforcements (both pending executions and active ones that need reversal)
async fn check_all_enforcements(http: &Http, data: &Data) -> Result<(), Error> {
    let now = Utc::now();

    // Find pending enforcements that need to be executed
    let mut enforcements_to_execute = Vec::new();
    for entry in &data.pending_enforcements {
        let pending = entry.value();
        if pending.state == EnforcementState::Pending {
            let execute_at = DateTime::parse_from_rfc3339(&pending.execute_at)
                .map_or_else(|_| DateTime::<Utc>::MIN_UTC, |dt| dt.with_timezone(&Utc));

            if execute_at <= now {
                enforcements_to_execute.push(pending.id.clone());
            }
        }
    }

    // Find active enforcements that need to be reversed
    let mut enforcements_to_reverse = Vec::new();
    for entry in &data.active_enforcements {
        let active = entry.value();
        if active.state == EnforcementState::Active && active.reverse_at.is_some() {
            if let Some(reverse_at_str) = &active.reverse_at {
                let reverse_at = DateTime::parse_from_rfc3339(reverse_at_str)
                    .map_or_else(|_| DateTime::<Utc>::MAX_UTC, |dt| dt.with_timezone(&Utc));

                if reverse_at <= now {
                    enforcements_to_reverse.push(active.id.clone());
                }
            }
        }
    }

    // Execute pending enforcements
    for id in &enforcements_to_execute {
        execute_enforcement(http, data, id).await?;
    }

    // Reverse active enforcements
    for id in &enforcements_to_reverse {
        reverse_enforcement(http, data, id).await?;
    }

    // Save updated data if anything was executed or reversed
    if !enforcements_to_execute.is_empty() || !enforcements_to_reverse.is_empty() {
        if let Err(e) = data.save().await {
            error!("Failed to save data after enforcement operations: {e}");
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
    let mut enforcements_to_reverse = Vec::new();

    // Find pending enforcements for this user in this guild
    for entry in &data.pending_enforcements {
        let pending = entry.value();
        if pending.state == EnforcementState::Pending
            && pending.user_id == user_id
            && pending.guild_id == guild_id
        {
            enforcements_to_execute.push(pending.id.clone());
        }
    }

    // Find active enforcements for this user that might need reversal
    for entry in &data.active_enforcements {
        let active = entry.value();
        if active.state == EnforcementState::Active
            && active.user_id == user_id
            && active.guild_id == guild_id
        {
            enforcements_to_reverse.push(active.id.clone());
        }
    }

    // Execute pending enforcements
    for id in &enforcements_to_execute {
        execute_enforcement(http, data, id).await?;
    }

    // Check if active enforcements should be reversed
    for id in &enforcements_to_reverse {
        // Only reverse if the time has come
        if let Some(active) = data.active_enforcements.get(id) {
            if let Some(reverse_at_str) = &active.reverse_at {
                if let Ok(reverse_at) = DateTime::parse_from_rfc3339(reverse_at_str) {
                    if reverse_at.with_timezone(&Utc) <= Utc::now() {
                        // Drop the borrow before calling reverse_enforcement
                        drop(active);
                        reverse_enforcement(http, data, id).await?;
                    }
                }
            }
        }
    }

    // Save updated data
    if !enforcements_to_execute.is_empty() || !enforcements_to_reverse.is_empty() {
        if let Err(e) = data.save().await {
            error!("Failed to save data after executing user enforcements: {e}");
        }
    }

    Ok(())
}

/// Check a specific enforcement by ID
async fn check_specific_enforcement(
    http: &Http,
    data: &Data,
    enforcement_id: &str,
) -> Result<(), Error> {
    // First check pending enforcements
    if let Some(pending) = data.pending_enforcements.get(enforcement_id) {
        if pending.state == EnforcementState::Pending {
            let id = pending.id.clone();
            drop(pending); // Drop the borrow before calling execute_enforcement
            execute_enforcement(http, data, &id).await?;

            // Save data
            if let Err(e) = data.save().await {
                error!("Failed to save data after executing specific enforcement: {e}");
            }
            return Ok(());
        }
    }

    // Then check active enforcements
    if let Some(active) = data.active_enforcements.get(enforcement_id) {
        // Only reverse if it's time
        if active.state == EnforcementState::Active && active.reverse_at.is_some() {
            let should_reverse = if let Some(reverse_at_str) = &active.reverse_at {
                if let Ok(reverse_at) = DateTime::parse_from_rfc3339(reverse_at_str) {
                    reverse_at.with_timezone(&Utc) <= Utc::now()
                } else {
                    false
                }
            } else {
                false
            };

            if should_reverse {
                let id = active.id.clone();
                drop(active); // Drop the borrow before calling reverse_enforcement
                reverse_enforcement(http, data, &id).await?;

                // Save data
                if let Err(e) = data.save().await {
                    error!("Failed to save data after reversing specific enforcement: {e}");
                }
                return Ok(());
            }
        }
    }

    // If we reach here, the enforcement wasn't found or doesn't need action
    warn!("Enforcement with ID {enforcement_id} not found or no action needed");
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
async fn _handle_ban_action(
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
async fn _handle_kick_action(
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
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
) -> Option<ChannelId> {
    // Fallback to HTTP if the guild is not in cache
    if let Ok(guild) = guild_id.to_partial_guild(http).await {
        // Get member to find the voice channel
        let voice_channels = get_guild_voice_channels(http.http(), &guild).await.ok()?;
        for channel_id in voice_channels {
            if let Ok(channel) = channel_id.to_channel(http.http()).await {
                let guild_channel = channel.guild()?;
                let res = http.cache().map(|cache| {
                    guild_channel
                        .members(cache)
                        .map(|members| members.iter().any(|member| member.user.id == user_id))
                });
                let res = res.unwrap_or_else(|| Ok(false));
                if res.unwrap_or(false) {
                    // User found in this channel
                    info!("User {user_id} is in voice channel {channel_id}");
                    return Some(channel_id);
                }
                // User not found in this channel, continue searching
            }
        }
    }
    None
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

/// Reverse an active enforcement action
async fn reverse_enforcement(http: &Http, data: &Data, enforcement_id: &str) -> Result<(), Error> {
    if let Some(mut active) = data.active_enforcements.get_mut(enforcement_id) {
        let guild_id = GuildId::new(active.guild_id);
        let user_id = UserId::new(active.user_id);
        let now = Utc::now().to_rfc3339();

        // Apply the reversal action based on the enforcement type
        match &active.action {
            EnforcementAction::Mute { .. } => {
                // For Discord timeouts, they're automatically removed by Discord
                // We just need to mark it as reversed in our system
                info!("Timeout period expired for user {user_id} in guild {guild_id}");
            }
            EnforcementAction::Ban { .. } => {
                // Unban the user
                info!("Unbanning user {user_id} in guild {guild_id}");
                match guild_id.unban(http, user_id).await {
                    Ok(()) => info!("Successfully unbanned user {user_id}"),
                    Err(e) => error!("Failed to unban user {user_id}: {e}"),
                }
            }
            EnforcementAction::VoiceMute { .. } => {
                // Remove voice mute
                info!("Removing voice mute from user {user_id} in guild {guild_id}");
                if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
                    match member.edit(http, EditMember::new().mute(false)).await {
                        Ok(()) => info!("Successfully removed voice mute from user {user_id}"),
                        Err(e) => error!("Failed to remove voice mute from user {user_id}: {e}"),
                    }
                }
            }
            EnforcementAction::VoiceDeafen { .. } => {
                // Remove voice deafen
                info!("Removing voice deafen from user {user_id} in guild {guild_id}");
                if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
                    match member.edit(http, EditMember::new().deafen(false)).await {
                        Ok(()) => info!("Successfully removed voice deafen from user {user_id}"),
                        Err(e) => error!("Failed to remove voice deafen from user {user_id}: {e}"),
                    }
                }
            }
            // These actions don't need reversal as they're one-time actions
            EnforcementAction::Kick { .. }
            | EnforcementAction::VoiceDisconnect { .. }
            | EnforcementAction::VoiceChannelHaunt { .. }
            | EnforcementAction::None => {}
        }

        // Update enforcement state
        active.state = EnforcementState::Reversed;
        active.reversed_at = Some(now);
        active.executed = true; // For backward compatibility

        // Get the enforcement ID and data for later
        let id = active.id.clone();
        let enforcement_data = active.value().clone();
        let user_id = enforcement_data.user_id;
        let guild_id = enforcement_data.guild_id;
        drop(active); // Drop the mutable borrow

        // Move enforcement to completed map
        data.active_enforcements.remove(&id);
        data.completed_enforcements
            .insert(id.clone(), enforcement_data);

        info!(
            target: crate::COMMAND_TARGET,
            enforcement_id = %id,
            user_id = %user_id,
            guild_id = %guild_id,
            event = "enforcement_reversed",
            "Enforcement action reversed"
        );
    } else {
        warn!("Active enforcement with ID {enforcement_id} not found for reversal");
    }

    Ok(())
}

/// Clear the pending enforcement from a user's warning state after it has been executed
fn clear_pending_enforcement(data: &Data, user_id: u64, guild_id: u64) {
    let key = format!("{user_id}:{guild_id}");

    if let Some(mut state) = data.user_warning_states.get_mut(&key) {
        // Clear the pending enforcement
        if state.pending_enforcement.is_some() {
            info!("Clearing pending enforcement for user {user_id} in guild {guild_id}");
            let mut updated_state = state.value().clone();
            updated_state.pending_enforcement = None;
            updated_state.last_updated = Utc::now().to_rfc3339();

            // Update the state
            *state = updated_state;
        }
    }
}

/// Execute a pending enforcement action
async fn execute_enforcement(http: &Http, data: &Data, enforcement_id: &str) -> Result<(), Error> {
    // Try to get and remove the pending enforcement
    if let Some(mut pending) = data.pending_enforcements.get_mut(enforcement_id) {
        let guild_id = GuildId::new(pending.guild_id);
        let user_id = UserId::new(pending.user_id);
        let now = Utc::now().to_rfc3339();

        // Execute the action based on the type
        match &pending.action {
            EnforcementAction::Mute { duration } => {
                handle_mute_action(http, guild_id, user_id, duration, false).await?;
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
                    false,
                )
                .await?;
            }
            EnforcementAction::Ban { duration } => {
                let duration = if let Some(dur) = duration { *dur } else { 0 };
                warn!(
                    target: crate::COMMAND_TARGET,
                    enforcement_id = %pending.id,
                    user_id = %user_id,
                    guild_id = %guild_id,
                    duration = %duration,
                    event = "enforcement_ban",
                    "*NOT* Executing ban action, don't be a pussy and uncomment this call"
                );
                //handle_ban_action(http, guild_id, user_id, duration, false).await?;
            }
            EnforcementAction::Kick { delay } => {
                let delay = if let Some(dur) = delay { *dur } else { 0 };
                warn!(
                    target: crate::COMMAND_TARGET,
                    enforcement_id = %pending.id,
                    user_id = %user_id,
                    guild_id = %guild_id,
                    delay = %delay,
                    event = "enforcement_kick",
                    "*NOT* Executing kick action, don't be a pussy and uncomment this call"
                );
                //handle_kick_action(http, guild_id, user_id, delay, false).await?;
            }
            EnforcementAction::VoiceMute { duration } => {
                handle_voice_mute_action(http, guild_id, user_id, duration, false).await?;
            }
            EnforcementAction::VoiceDeafen { duration } => {
                handle_voice_deafen_action(http, guild_id, user_id, duration, false).await?;
            }
            EnforcementAction::VoiceDisconnect { delay } => {
                handle_voice_disconnect_action(http, guild_id, user_id, delay.as_ref(), false)
                    .await?;
            }
            EnforcementAction::None => {}
        }

        // Calculate when to reverse the action (if applicable)
        let reverse_at_option = calculate_reversal_time(&pending.action);

        // Determine if this is a one-time action
        let is_one_time = matches!(
            &pending.action,
            EnforcementAction::Kick { .. }
                | EnforcementAction::VoiceDisconnect { .. }
                | EnforcementAction::VoiceChannelHaunt { .. }
        );

        // Determine final state based on the action type and reverse_at
        let needs_reversal = !is_one_time && reverse_at_option.is_some();

        // Get the enforcement data we need for logging
        let id = pending.id.clone();
        let user_id = pending.user_id;
        let guild_id = pending.guild_id;

        // Update enforcement state
        pending.state = EnforcementState::Active;
        pending.executed_at = Some(now.clone());
        pending.executed = true; // For backward compatibility
        pending.reverse_at.clone_from(&reverse_at_option);

        // Clone the enforcement data before dropping the borrow
        let mut enforcement_data = pending.value().clone();
        drop(pending); // Drop the mutable borrow

        // Remove from pending enforcements
        data.pending_enforcements.remove(&id);

        // Determine where to put the enforcement based on whether it needs reversal
        if needs_reversal {
            // For actions that will need reversal, move to active
            data.active_enforcements
                .insert(id.clone(), enforcement_data);
        } else {
            // For actions that don't need reversal, move directly to completed
            enforcement_data.state = EnforcementState::Completed;
            data.completed_enforcements
                .insert(id.clone(), enforcement_data);
        }

        // Clear the pending enforcement from the user's warning state so future warnings
        // can set new appropriate enforcements based on the infraction type
        clear_pending_enforcement(data, user_id, guild_id);

        info!(
            target: crate::COMMAND_TARGET,
            enforcement_id = %id,
            user_id = %user_id,
            guild_id = %guild_id,
            event = "enforcement_executed",
            "Enforcement action executed"
        );
    } else {
        warn!("Pending enforcement with ID {enforcement_id} not found");
    }

    Ok(())
}

/// Calculate when an enforcement action should be reversed
fn calculate_reversal_time(action: &EnforcementAction) -> Option<String> {
    match action {
        EnforcementAction::Mute { duration }
        | EnforcementAction::Ban { duration }
        | EnforcementAction::VoiceMute { duration }
        | EnforcementAction::VoiceDeafen { duration } => {
            if let Some(secs) = duration {
                if *secs > 0 {
                    // Add duration to current time
                    Some((Utc::now() + chrono::Duration::seconds(*secs as i64)).to_rfc3339())
                } else {
                    None
                }
            } else {
                None
            }
        }
        // These actions don't require reversal as they're one-time operations
        EnforcementAction::Kick { .. }
        | EnforcementAction::VoiceDisconnect { .. }
        | EnforcementAction::VoiceChannelHaunt { .. }
        | EnforcementAction::None => None,
    }
}

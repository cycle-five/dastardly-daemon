use crate::{
    Data, Error,
    data::{
        EnforcementAction, GuildConfig, NotificationMethod, PendingEnforcement, UserWarningState,
        Warning, WarningContext,
    },
    enforcement::EnforcementCheckRequest,
};
use chrono::{Duration, Utc};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{Colour, CreateEmbed, CreateMessage, Mentionable, Timestamp, User};
use poise::{Context, command};
use tracing::{error, info, warn};
use uuid::Uuid;
// Determine if enforcement should be triggered
// Threshold is 2.0 (roughly 2 recent warnings)
const WARNING_THRESHOLD: f64 = 2.0;

/// Basic ping command
/// This command is used to check if the bot is responsive.
#[command(slash_command, guild_only)]
pub async fn ping(ctx: Context<'_, Data, Error>) -> Result<(), Error> {
    ctx.say("Pong!").await?;
    Ok(())
}

/// Summon the daemon to judge a user's behavior and apply appropriate consequences
#[command(
    slash_command,
    guild_only,
    ephemeral,
    required_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS|SEND_MESSAGES",
    required_bot_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS|SEND_MESSAGES",
    default_member_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS|SEND_MESSAGES"
)]
pub async fn summon_daemon(
    ctx: Context<'_, Data, Error>,
    #[description = "User to warn"] user: User,
    #[description = "Reason for warning"] reason: String,
    #[description = "Infraction type (text, voice, server)"] infraction_type: Option<String>,
    #[description = "Notification method (dm, public)"] notification: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    // Get guild configuration
    let guild_config = get_guild_config(&ctx, guild_id);

    // Determine infraction category
    let infraction_type = infraction_type
        .unwrap_or_else(|| "voice".to_string())
        .to_lowercase();

    // Determine notification method
    let notification_method = match notification.as_deref() {
        Some("dm" | "DM") => NotificationMethod::DirectMessage,
        Some("public" | "Public") => NotificationMethod::PublicWithMention,
        _ => guild_config.default_notification_method,
    };

    // Record this warning in the user's warning state
    let user_id = user.id.get();
    let mod_id = ctx.author().id.get();
    let state =
        ctx.data()
            .add_to_user_warning_state(user_id, guild_id.get(), reason.clone(), mod_id);

    // Calculate the warning score
    let score = ctx.data().calculate_warning_score(user_id, guild_id.get());

    // Add randomness based on the chaos factor
    let random_factor: f64 = {
        let mut rng = rand::thread_rng();
        rand::Rng::gen_range(&mut rng, 0.0..f64::from(guild_config.chaos_factor))
    };
    let adjusted_score = score + random_factor;

    let enforce = adjusted_score > WARNING_THRESHOLD;
    let enforcement_action = if state.pending_enforcement.is_some() {
        // Use the pending enforcement that was set on first warning
        state.pending_enforcement.clone()
    } else if state.warning_timestamps.len() == 1 {
        // This is the first warning, set a pending enforcement based on infraction type
        let enforcement = match infraction_type.as_str() {
            "voice" => guild_config
                .default_enforcement
                .unwrap_or(EnforcementAction::VoiceMute {
                    duration: Some(300),
                }),
            "server" => guild_config
                .default_enforcement
                .unwrap_or(EnforcementAction::Kick { delay: Some(0) }),
            _ => guild_config // text
                .default_enforcement
                .unwrap_or(EnforcementAction::Mute {
                    duration: Some(300),
                }),
        };

        // Store the pending enforcement in the user state
        let key = format!("{}:{}", user_id, guild_id.get());
        let mut updated_state = state.clone();
        updated_state.pending_enforcement = Some(enforcement.clone());
        ctx.data().user_warning_states.insert(key, updated_state);

        Some(enforcement)
    } else {
        None
    };

    // Create a warning UUID for tracking
    let warning_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Create a formal warning record
    let warning = Warning {
        id: warning_id.clone(),
        user_id,
        issuer_id: mod_id,
        guild_id: guild_id.get(),
        reason: reason.clone(),
        timestamp: now.clone(),
        notification_method: notification_method.clone(),
        enforcement: enforcement_action.clone(),
    };

    // Store warning
    ctx.data().warnings.insert(warning_id.clone(), warning);

    // Generate a demonic response based on the context
    let is_voice = infraction_type == "voice";
    let response_type = if enforce {
        crate::llm::ResponseType::Punishment
    } else if state.warning_timestamps.len() == 1 {
        crate::llm::ResponseType::Summoning
    } else {
        crate::llm::ResponseType::Warning
    };

    // Create context for LLM
    // let context = format!(
    //     "User: {}. Infraction: {}. Reason: {}. Warning count: {}. Score: {:.2}. Enforcing: {}.",
    //     user.name,
    //     infraction_type,
    //     reason,
    //     state.warning_timestamps.len(),
    //     adjusted_score,
    //     enforce
    // );
    let warning_context = WarningContext {
        user_name: user.name.clone(),
        num_warn: state.warning_timestamps.len() as u64,
        voice_warnings: ctx.data().get_warnings(),
        warning_score: adjusted_score,
        warning_threshold: WARNING_THRESHOLD,
        mod_name: ctx.author().name.clone(),
    };

    // Generate a demonic message based on the context
    let demonic_message = generate_daemon_response(&warning_context.to_string(), Some(&state), response_type).await;

    // Notify the user via the enforcement log channel
    if let Some(log_channel_id) = guild_config.enforcement_log_channel_id {
        log_daemon_warning(
            &ctx,
            log_channel_id,
            &user,
            &reason,
            &infraction_type,
            &state,
            &enforcement_action,
            enforce,
            &demonic_message,
        )
        .await;
    }

    // Notify user based on notification method and infraction type
    match notification_method {
        NotificationMethod::DirectMessage => {
            if let Ok(channel) = user.create_dm_channel(&ctx.http()).await {
                // For voice infractions, use a more natural demonic message without embeds
                if is_voice {
                    let message = CreateMessage::new().content(format!(
                        "**[DAEMON WHISPERS]** {}\n\nYou have been warned in {}",
                        demonic_message,
                        ctx.guild().unwrap().name,
                    ));
                    let _ = channel.send_message(&ctx.http(), message).await;
                } else {
                    // For non-voice infractions, use a simpler format but still include the demonic message
                    let message = CreateMessage::new().content(format!(
                        "**[DAEMON SPEAKS]** {}\n\nYou have been warned in {}",
                        demonic_message,
                        ctx.guild().unwrap().name,
                    ));
                    let _ = channel.send_message(&ctx.http(), message).await;
                }
            }
        }
        NotificationMethod::PublicWithMention => {
            // For voice infractions, use a more natural demonic message without embeds
            if is_voice {
                let content = format!(
                    "**[DAEMON ROARS]** {}\n\n{}",
                    demonic_message,
                    user.mention()
                );
                let _ = ctx.say(content).await;
            } else {
                // For non-voice infractions, use a simpler format but still include the demonic message
                let content = format!(
                    "**[DAEMON DECLARES]** {}\n\n{}",
                    demonic_message,
                    user.mention(),
                );
                let _ = ctx.say(content).await;
            }
        }
    }

    // If enforcing, create or update the enforcement
    if enforce && enforcement_action.is_some() {
        // Create pending enforcement and notify
        if let Some(action) = enforcement_action {
            create_and_notify_enforcement(&ctx, warning_id, user_id, guild_id.get(), action).await;
        }
    }

    // Save data
    let _ = save_data(&ctx, "daemon summon").await;

    // Respond to the moderator
    let response = if enforce {
        format!(
            "Summon recorded for {} with reason: {}. The daemon shall execute judgment!",
            user.name, reason
        )
    } else if state.warning_timestamps.len() == 1 {
        format!(
            "First summoning recorded for {} with reason: {}. The daemon is watching...",
            user.name, reason
        )
    } else {
        format!(
            "Summon recorded for {} with reason: {}. Current warning count: {}. The daemon grows restless...",
            user.name,
            reason,
            state.warning_timestamps.len()
        )
    };

    ctx.say(response).await?;
    Ok(())
}

/// Generate a demonic response based on the context.
/// This should be used to create thematic messages for the daemon via
/// the LLM integration.
async fn generate_daemon_response(
    warning_context: &str,
    state: Option<&UserWarningState>,
    response_type: crate::llm::ResponseType,
) -> String {
    #[cfg(feature = "llm-integration")]
    {
        crate::llm::generate_daemon_response(
            warning_context,
            state,
            response_type,
        )
        .await
    }
    #[cfg(not(feature = "llm-integration"))]
    {
        let _ = state;
        let _ = response_type;
        warning_context.to_string()
    }
}

/// [DEPRECATED] Warn a user for inappropriate behavior.
/// Please use `/summon_daemon` instead.
#[allow(clippy::too_many_lines)]
#[command(
    slash_command,
    ephemeral,
    guild_only,
    required_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS",
    required_bot_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS",
    default_member_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS"
)]
pub async fn warn(
    ctx: Context<'_, Data, Error>,
    #[description = "User to warn"] user: User,
    #[description = "Reason for warning"] reason: String,
    #[description = "Notification method (DM or Public)"] notification: Option<String>,
    #[description = "Action to take (mute, ban, kick, voicemute, voicedeafen, voicedisconnect)"]
    action: Option<String>,
    #[description = "Duration in minutes for mute/ban/voicemute/voicedeafen, delay for kick/voicedisconnect"]
    duration_minutes: Option<u64>,
) -> Result<(), Error> {
    // Show deprecation notice
    ctx.say("⚠️ This command is deprecated. Please use `/summon_daemon` instead for improved functionality.").await?;
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    // Get guild configuration
    let guild_config = get_guild_config(&ctx, guild_id);

    // Determine notification method
    let notification_method = match notification.as_deref() {
        Some("public" | "Public") => NotificationMethod::PublicWithMention,
        Some("dm" | "DM") => NotificationMethod::DirectMessage,
        _ => guild_config.default_notification_method,
    };

    // Determine enforcement action
    let duration = duration_minutes.map(|d| d * 60);
    let enforcement = match action.as_deref() {
        Some("mute" | "Mute") => Some(EnforcementAction::Mute { duration }),
        Some("ban" | "Ban") => Some(EnforcementAction::Ban { duration }),
        Some("kick" | "Kick") => Some(EnforcementAction::Kick { delay: duration }),
        Some("voicemute" | "VoiceMute") => Some(EnforcementAction::VoiceMute { duration }),
        Some("voicedeafen" | "VoiceDeafen") => Some(EnforcementAction::VoiceDeafen { duration }),
        Some("voicedisconnect" | "VoiceDisconnect") => {
            Some(EnforcementAction::VoiceDisconnect { delay: duration })
        }
        _ => guild_config.default_enforcement,
    };

    warn!("Enforcement action: {enforcement:?}");

    // Create warning
    let warning_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let warning = Warning {
        id: warning_id.clone(),
        user_id: user.id.get(),
        issuer_id: ctx.author().id.get(),
        guild_id: guild_id.get(),
        reason,
        timestamp: now.clone(),
        notification_method,
        enforcement: enforcement.clone(),
    };

    // Store warning
    ctx.data()
        .warnings
        .insert(warning_id.clone(), warning.clone());

    // Create pending enforcement if applicable
    if let Some(action) = enforcement {
        let enforcement_id = create_pending_enforcement(
            &ctx,
            warning_id.clone(),
            user.id.get(),
            guild_id.get(),
            action,
        )
        .await;
        info!("Pending enforcement created with ID: {}", enforcement_id);
        info!(
            "Pending enforcements: {:?}",
            ctx.data().pending_enforcements
        );
    }

    // Notify user based on notification method
    match warning.notification_method {
        NotificationMethod::DirectMessage => {
            if let Ok(channel) = user.create_dm_channel(&ctx.http()).await {
                let embed = CreateEmbed::new()
                    .title("Warning Received")
                    .description(format!(
                        "You have been warned in {} for: {}",
                        ctx.guild().unwrap().name,
                        warning.reason
                    ))
                    .colour(Colour::RED)
                    .timestamp(Timestamp::now());

                let message = CreateMessage::new().embed(embed);
                let _ = channel.send_message(&ctx.http(), message).await;
            }
        }
        NotificationMethod::PublicWithMention => {
            let content = format!(
                "{} You have been warned for: {}",
                user.mention(),
                warning.reason
            );
            let embed = CreateEmbed::new()
                .title("Warning Issued")
                .description(&content)
                .colour(Colour::RED)
                .timestamp(Timestamp::now());

            ctx.send(poise::CreateReply::default().embed(embed)).await?;
        }
    }

    // Log the warning
    info!(
        target: crate::COMMAND_TARGET,
        command = "warn",
        guild_id = %guild_id.get(),
        user_id = %user.id.get(),
        issuer_id = %ctx.author().id.get(),
        reason = %warning.reason,
        event = "warning_issued",
        "Warning issued to user"
    );

    // Save data
    let _ = save_data(&ctx, "warning").await;

    info!(
        target: crate::COMMAND_TARGET,
        command = "warn",
        guild_id = %guild_id.get(),
        user_id = %user.id.get(),
        issuer_id = %ctx.author().id.get(),
        reason = %warning.reason,
        event = "warning_saved",
        "Warning saved to database"
    );

    // If there's an immediate action, notify the enforcement task
    if let Some(action) = &warning.enforcement {
        info!(
            target: crate::COMMAND_TARGET,
            command = "warn",
            guild_id = %guild_id.get(),
            user_id = %user.id.get(),
            issuer_id = %ctx.author().id.get(),
            reason = %warning.reason,
            event = "immediate_enforcement_check",
            enforcement_action = ?action,
            "Immediate enforcement action detected"
        );

        if is_immediate_action(action) {
            info!(
                target: crate::COMMAND_TARGET,
                command = "warn",
                guild_id = %guild_id.get(),
                user_id = %user.id.get(),
                issuer_id = %ctx.author().id.get(),
                event = "immediate_enforcement_request",
                "Sending immediate enforcement check request"
            );
            // For immediate actions, notify the enforcement task
            notify_enforcement_task(&ctx, user.id.get(), guild_id.get()).await;
        } else {
            warn!("Enforcement action is not immediate: {action:?}");
            // Non-immediate actions will be handled by the regular check interval
        }
    }

    ctx.say(format!("Warned {} for: {}", user.name, warning.reason))
        .await?;
    Ok(())
}

/// Set the altar where the daemon will send its messages
#[command(
    slash_command,
    guild_only,
    ephemeral,
    required_permissions = "ADMINISTRATOR"
)]
pub async fn daemon_altar(
    ctx: Context<'_, Data, Error>,
    #[description = "Channel to use for enforcement logs"] channel: serenity::Channel,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    // Get current guild config or create default
    let mut guild_config = get_guild_config(&ctx, guild_id);

    // Remember old channel if any
    let old_channel_id = guild_config.enforcement_log_channel_id;

    // Update the config with the new channel ID
    let channel_id = channel.id();
    guild_config.enforcement_log_channel_id = Some(channel_id.get());

    // Save the updated config
    ctx.data().guild_configs.insert(guild_id, guild_config);

    // Generate a demonic response for the altar setting
    let context = format!(
        "Admin: {}. Setting new altar channel: {}. Old channel: {}.",
        ctx.author().name,
        channel.mention(),
        old_channel_id.map_or("none".to_string(), |id| id.to_string())
    );

    let demonic_message =
        generate_daemon_response(&context, None, crate::llm::ResponseType::Summoning).await;

    // Save data
    if (save_data(&ctx, "setting enforcement log channel").await).is_err() {
        ctx.say("Failed to save configuration. Check logs for details.")
            .await?;
        return Ok(());
    }

    // Send a test message to verify permissions
    let test_message = format!(
        "**[DAEMON ALTAR ESTABLISHED]**\n\n{demonic_message}\n\nThis channel shall serve as my altar. All warnings, judgments, and enforcements shall be proclaimed here.",
    );

    let message = serenity::CreateMessage::new().content(test_message);

    match channel_id.send_message(&ctx.http(), message).await {
        Ok(_) => {
            ctx.say(format!(
                "**[DAEMON ALTAR SET]** The daemon's altar has been established in {}. It will now receive all proclamations and judgments.",
                channel.mention()
            ))
            .await?;
        }
        Err(e) => {
            error!("Failed to send test message to channel: {}", e);
            ctx.say(format!(
                "**[DAEMON DISPLEASED]** The altar was set to {}, but the daemon cannot speak there. Check channel permissions immediately!",
                channel.mention()
            ))
            .await?;
        }
    }

    Ok(())
}

/// Perform a ritual to adjust the daemon's chaos level
#[command(
    slash_command,
    guild_only,
    ephemeral,
    required_permissions = "ADMINISTRATOR"
)]
pub async fn chaos_ritual(
    ctx: Context<'_, Data, Error>,
    #[description = "Chaos factor (0.0-1.0) where higher means more random"] factor: f32,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    if !(0.0..=1.0).contains(&factor) {
        ctx.say("Chaos factor must be between 0.0 and 1.0").await?;
        return Ok(());
    }

    // Get current guild config or create default
    let mut guild_config = get_guild_config(&ctx, guild_id);

    // Get previous factor to determine if increasing or decreasing
    let previous_factor = guild_config.chaos_factor;
    let is_increasing = factor > previous_factor;

    // Update the chaos factor
    guild_config.chaos_factor = factor;

    // Save the updated config
    ctx.data()
        .guild_configs
        .insert(guild_id, guild_config.clone());

    // Generate a demonic response for the chaos ritual
    let context = format!(
        "Chaos factor changed from {:.2} to {:.2}. Is increasing: {}. Moderator: {}.",
        previous_factor,
        factor,
        is_increasing,
        ctx.author().name
    );

    let demonic_message =
        generate_daemon_response(&context, None, crate::llm::ResponseType::ChaosRitual).await;

    // Create a more thematic message based on the chaos level
    let ritual_status = if factor < 0.2 {
        "The daemon's powers become focused and controlled."
    } else if factor < 0.5 {
        "The daemon grows restless with chaotic potential."
    } else if factor < 0.8 {
        "The daemon's unpredictability intensifies."
    } else {
        "The daemon's power reaches its most chaotic state!"
    };

    // Create a response that combines the daemon's voice with information
    let response = format!(
        "**[DAEMON RITUAL COMPLETE]** {demonic_message}\n\nChaos factor set to {factor:.2}. {ritual_status}",
    );

    // Save data
    if (save_data(&ctx, "setting chaos factor").await).is_err() {
        ctx.say("Failed to save configuration. Check logs for details.")
            .await?;
        return Ok(());
    }

    // If there's a log channel, also log the ritual there
    if let Some(log_channel_id) = guild_config.enforcement_log_channel_id {
        let content = format!(
            "🔮 **CHAOS RITUAL PERFORMED**\n\n{}\n\nRitual performed by: {}\nChaos Factor: {:.2}\n\n{}",
            demonic_message,
            ctx.author().mention(),
            factor,
            ritual_status
        );

        let channel_id = serenity::ChannelId::new(log_channel_id);
        let message = serenity::CreateMessage::new().content(content);
        let _ = channel_id.send_message(&ctx.http(), message).await;
    }

    ctx.say(response).await?;
    Ok(())
}

/// View a user's warning history and current warning score
#[command(
    slash_command,
    guild_only,
    ephemeral,
    required_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS",
    required_bot_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS",
    default_member_permissions = "KICK_MEMBERS|BAN_MEMBERS|MUTE_MEMBERS|DEAFEN_MEMBERS|MODERATE_MEMBERS"
)]
pub async fn judgment_history(
    ctx: Context<'_, Data, Error>,
    #[description = "User to check"] user: User,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;

    let user_id = user.id.get();

    // Get the user's warning state
    let state = ctx
        .data()
        .get_or_create_user_warning_state(user_id, guild_id.get());

    // Get all warnings for this user in this guild
    let mut warnings = Vec::new();
    let mut voice_warnings = 0;

    for entry in &ctx.data().warnings {
        let warning = entry.value();
        if warning.user_id == user_id && warning.guild_id == guild_id.get() {
            // Check if it's a voice-related warning based on enforcement action
            if let Some(action) = &warning.enforcement {
                if matches!(
                    action,
                    EnforcementAction::VoiceMute { .. }
                        | EnforcementAction::VoiceDeafen { .. }
                        | EnforcementAction::VoiceDisconnect { .. }
                        | EnforcementAction::VoiceChannelHaunt { .. }
                ) {
                    voice_warnings += 1;
                }
            }
            warnings.push(warning.clone());
        }
    }

    // Sort warnings by timestamp (newest first)
    warnings.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Get the current warning score
    let score = ctx.data().calculate_warning_score(user_id, guild_id.get());

    // Generate a demonic response for the judgment history
    let warn_context = WarningContext {
        user_name: user.name.clone(),
        num_warn: warnings.len() as u64,
        voice_warnings: warnings.clone(),
        warning_score: score,
        warning_threshold: WARNING_THRESHOLD,
        mod_name: ctx.author().name.clone(),
    };

    // Use a punishment type if close to threshold, otherwise warning type
    let response_type = if score > WARNING_THRESHOLD * 0.75 {
        crate::llm::ResponseType::Punishment
    } else {
        crate::llm::ResponseType::Warning
    };

    let demonic_message =
        generate_daemon_response(&warn_context.to_string(), Some(&state), response_type).await;

    // Create thematic header based on warning score
    let header = if score > WARNING_THRESHOLD {
        "**[DAEMON JUDGMENT SCROLL - CONDEMNED]**"
    } else if score > WARNING_THRESHOLD * 0.75 {
        "**[DAEMON JUDGMENT SCROLL - TEETERING]**"
    } else if score > WARNING_THRESHOLD * 0.5 {
        "**[DAEMON JUDGMENT SCROLL - CONCERNING]**"
    } else if score > 0.0 {
        "**[DAEMON JUDGMENT SCROLL - NOTED]**"
    } else {
        "**[DAEMON JUDGMENT SCROLL - UNBLEMISHED]**"
    };

    // Determine if there are voice infractions
    let has_voice_infractions = voice_warnings > 0;

    // Build a message content instead of an embed for more natural daemon speech
    let mut content = format!(
        "{}\n\n{}\n\n{} has **{}** warnings with a current judgment score of **{:.2}/{:.1}**.\n",
        header,
        demonic_message,
        user.mention(),
        state.warning_timestamps.len(),
        score,
        WARNING_THRESHOLD
    );

    // Add pending enforcement if any
    if let Some(action) = &state.pending_enforcement {
        let action_desc = match action {
            EnforcementAction::VoiceMute { duration } => {
                format!(
                    "voice shall be silenced for {} seconds",
                    duration.unwrap_or(300)
                )
            }
            EnforcementAction::VoiceDeafen { duration } => {
                format!(
                    "ears shall be cursed for {} seconds",
                    duration.unwrap_or(300)
                )
            }
            EnforcementAction::VoiceDisconnect { .. } => {
                "mortal shall be banished from the voice realm".to_string()
            }
            EnforcementAction::Mute { duration } => {
                format!(
                    "text shall be silenced for {} seconds",
                    duration.unwrap_or(300)
                )
            }
            EnforcementAction::Ban { duration } => {
                format!("banishment for {} seconds", duration.unwrap_or(86400))
            }
            EnforcementAction::Kick { .. } => "exile from the realm".to_string(),
            EnforcementAction::None => "no action".to_string(),
            EnforcementAction::VoiceChannelHaunt { .. } => {
                "haunting through the voice channels".to_string()
            }
        };

        content.push_str(&format!(
            "\n**PENDING JUDGMENT**: Should the mortal's score exceed {WARNING_THRESHOLD:.1}, their fate shall be: **{action_desc}**\n",
        ));
    }

    // Add recent warnings
    content.push_str("\n**RECORDED TRANSGRESSIONS**:\n");

    if warnings.is_empty() {
        content.push_str("No transgressions recorded... yet.\n");
    } else {
        for (i, warning) in warnings.iter().take(10).enumerate() {
            let timestamp = warning
                .timestamp
                .split('T')
                .next()
                .unwrap_or(&warning.timestamp);
            let issuer = ctx
                .http()
                .get_user(warning.issuer_id.into())
                .await
                .map(|u| u.name.clone())
                .unwrap_or_else(|_| "Unknown Moderator".to_string());

            content.push_str(&format!(
                "{}. **{}**: {} (Reported by {})\n",
                i + 1,
                timestamp,
                warning.reason,
                issuer
            ));
        }

        if warnings.len() > 10 {
            content.push_str(&format!(
                "\n{} additional transgressions remain sealed in the ancient scrolls...\n",
                warnings.len() - 10
            ));
        }
    }

    // Add a thematic closing
    if has_voice_infractions {
        content.push_str("\n*The daemon remembers all voices that have disturbed its realm...*");
    } else {
        content.push_str("\n*The daemon's all-seeing eye continues to watch...*");
    }

    ctx.say(content).await?;
    Ok(())
}

/// Appease the daemon to cancel a pending punishment
#[command(
    slash_command,
    guild_only,
    ephemeral,
    required_permissions = "ADMINISTRATOR"
)]
pub async fn appease(
    ctx: Context<'_, Data, Error>,
    #[description = "User whose enforcement to cancel"] user: User,
    #[description = "Specific enforcement ID to cancel (optional)"] enforcement_id: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx
        .guild_id()
        .ok_or("This command must be used in a guild")?;
    let user_id = user.id.get();
    let mut canceled = false;
    let mut canceled_enforcements = Vec::new();

    // Find pending enforcements for this user in this guild
    let mut pending_to_cancel = Vec::new();
    for entry in &ctx.data().pending_enforcements {
        let pending = entry.value();
        if pending.user_id == user_id && pending.guild_id == guild_id.get() && !pending.executed {
            if let Some(ref eid) = enforcement_id {
                if pending.id == *eid {
                    pending_to_cancel.push(pending.id.clone());
                    canceled_enforcements.push(pending.clone());
                    break;
                }
            } else {
                pending_to_cancel.push(pending.id.clone());
                canceled_enforcements.push(pending.clone());
            }
        }
    }

    // Cancel the found enforcements
    for id in pending_to_cancel {
        if let Some(mut pending) = ctx.data().pending_enforcements.get_mut(&id) {
            pending.executed = true;
            canceled = true;

            // Notify the enforcement task that this enforcement has been canceled
            notify_enforcement_task_by_id(&ctx, id.clone()).await;
        }
    }

    // Get user state if available
    let user_state = ctx
        .data()
        .get_or_create_user_warning_state(user_id, guild_id.get());

    // Generate a demonic appeasement response
    let context = format!(
        "User: {}. Enforcements canceled: {}. Moderator: {}.",
        user.name,
        canceled_enforcements.len(),
        ctx.author().name
    );

    let demonic_message = generate_daemon_response(
        &context,
        Some(&user_state),
        crate::llm::ResponseType::Appeasement,
    )
    .await;

    if canceled {
        // Check if any of the canceled enforcements involved voice
        let has_voice_enforcement = canceled_enforcements.iter().any(|enforcement| {
            matches!(
                enforcement.action,
                EnforcementAction::VoiceMute { .. }
                    | EnforcementAction::VoiceDeafen { .. }
                    | EnforcementAction::VoiceDisconnect { .. }
                    | EnforcementAction::VoiceChannelHaunt { .. }
            )
        });

        // Format response based on whether it's voice-related
        let response = if has_voice_enforcement {
            format!(
                "**[DAEMON RELUCTANTLY YIELDS]** {}\n\nThe daemon has been appeased. Pending punishment for {} has been canceled.",
                demonic_message, user.name
            )
        } else {
            format!(
                "**[DAEMON GRUMBLES]** {}\n\nThe daemon has been appeased. Pending punishment for {} has been canceled.",
                demonic_message, user.name
            )
        };

        // Save data
        let _ = save_data(&ctx, "canceling enforcement").await;

        ctx.say(response).await?;
    } else {
        ctx.say(format!("No pending enforcements found for {}", user.name))
            .await?;
    }

    Ok(())
}

/// Logs a daemon warning/enforcement to the guild's log channel
async fn log_daemon_warning(
    ctx: &Context<'_, Data, Error>,
    log_channel_id: u64,
    user: &User,
    reason: &str,
    infraction_type: &str,
    state: &crate::data::UserWarningState,
    enforcement_action: &Option<EnforcementAction>,
    enforce: bool,
    demonic_message: &str,
) {
    let channel_id = serenity::ChannelId::new(log_channel_id);
    let user_mention = user.mention();
    let mod_mention = ctx.author().mention();
    let warning_count = state.warning_timestamps.len();
    let warning_score = ctx
        .data()
        .calculate_warning_score(user.id.get(), state.guild_id);

    // Focus on voice chat cases with a more natural, demonic tone
    if infraction_type == "voice" {
        // For voice infractions, use a more natural message format
        let status_emoji = if enforce { "🔥" } else { "👁️" };
        let (header, _color_hex) = if enforce {
            ("**DAEMON ENFORCEMENT ACTIVATED**", "#FF0000")
        } else if state.warning_timestamps.len() == 1 {
            ("**DAEMON SUMMONED**", "#FFA500")
        } else {
            ("**DAEMON WARNING**", "#FFCC00")
        };

        // Create action description if applicable
        let action_text = if let Some(action) = enforcement_action {
            match action {
                EnforcementAction::VoiceMute { duration } => {
                    format!(
                        "Voice shall be silenced for {} seconds",
                        duration.unwrap_or(300)
                    )
                }
                EnforcementAction::VoiceDeafen { duration } => {
                    format!(
                        "Ears shall be cursed for {} seconds",
                        duration.unwrap_or(300)
                    )
                }
                EnforcementAction::VoiceDisconnect { .. } => {
                    "Mortal shall be banished from the voice realm".to_string()
                }
                EnforcementAction::VoiceChannelHaunt { .. } => {
                    "Mortal shall be haunted through the voice channels".to_string()
                }
                _ => format!("Punishment: {action:?}"),
            }
        } else {
            "No immediate action... for now".to_string()
        };

        // Create content message in a demonic style
        let content = format!(
            "{} {}\n\n{}\n\n{} has disturbed the voice channels.\nReason: {}\nSummoned by: {}\nWarning count: {} (Score: {:.2}/{:.1})\n\n{}",
            status_emoji,
            header,
            demonic_message,
            user_mention,
            reason,
            mod_mention,
            warning_count,
            warning_score,
            WARNING_THRESHOLD,
            if enforce {
                format!("**JUDGMENT**: {action_text}")
            } else if state.warning_timestamps.len() == 1 {
                format!("**WARNING**: Further transgressions will result in: {action_text}",)
            } else {
                format!(
                    "**WARNING**: The daemon grows restless. Score approaches threshold of {WARNING_THRESHOLD:.1}",
                )
            }
        );

        let message = serenity::CreateMessage::new().content(content);
        let _ = channel_id.send_message(&ctx.http(), message).await;
    } else {
        // For non-voice infractions, use a hybrid approach with an embed for the log
        let (title_prefix, emoji) = match infraction_type {
            "text" => ("Text Channel", "💬"),
            "server" => ("Server Rule", "⚠️"),
            _ => ("General", "⚠️"),
        };

        let title = if enforce {
            format!("🚫 {title_prefix} Enforcement")
        } else {
            format!("{emoji} {title_prefix} Warning")
        };

        let mut embed = serenity::CreateEmbed::new()
            .title(title)
            .description(format!(
                "{demonic_message}\n\n{user_mention} has received a {infraction_type} warning",
            ))
            .field("Reason", reason, false)
            .field("Issued By", mod_mention.to_string(), true)
            .field("Total Warnings", warning_count.to_string(), true)
            .field("Warning Score", format!("{warning_score:.2}"), true)
            .colour(serenity::Colour::GOLD)
            .timestamp(serenity::Timestamp::now());

        // If this might lead to enforcement, indicate that
        if let Some(action) = enforcement_action {
            if state.warning_timestamps.len() == 1 {
                // This is the first warning, indicate what will happen
                let action_desc = match action {
                    EnforcementAction::VoiceMute { duration } => {
                        format!("Voice mute for {} seconds", duration.unwrap_or(300))
                    }
                    EnforcementAction::VoiceDeafen { duration } => {
                        format!("Voice deafen for {} seconds", duration.unwrap_or(300))
                    }
                    EnforcementAction::VoiceDisconnect { .. } => "Voice disconnect".to_string(),
                    EnforcementAction::Mute { duration } => {
                        format!("Server mute for {} seconds", duration.unwrap_or(300))
                    }
                    EnforcementAction::Ban { duration } => {
                        format!("Ban for {} seconds", duration.unwrap_or(86400))
                    }
                    EnforcementAction::Kick { .. } => "Kick".to_string(),
                    EnforcementAction::None => "No action".to_string(),
                    EnforcementAction::VoiceChannelHaunt {
                        teleport_count,
                        interval,
                        return_to_origin,
                        ..
                    } => {
                        format!(
                            "Voice channel haunting: {} teleports over {} seconds{}",
                            teleport_count.unwrap_or(3),
                            interval.unwrap_or(10),
                            if return_to_origin.unwrap_or(true) {
                                " (with return)"
                            } else {
                                " (no return)"
                            }
                        )
                    }
                };

                embed = embed.field(
                    "🚨 If behavior continues:",
                    format!(
                        "After reaching a warning score of {WARNING_THRESHOLD:.1}, the user will receive: **{action_desc}**",
                    ),
                    false,
                );
            } else if enforce {
                // Enforcement is happening now
                embed = embed.colour(serenity::Colour::RED).field(
                    "⚠️ Threshold Reached",
                    "The daemon has been summoned. Enforcement action is being applied.",
                    false,
                );
            }
        }

        let message = serenity::CreateMessage::new().embed(embed);
        let _ = channel_id.send_message(&ctx.http(), message).await;
    }
}

/// Retrieves the guild configuration or creates a default one
fn get_guild_config(ctx: &Context<'_, Data, Error>, guild_id: serenity::GuildId) -> GuildConfig {
    ctx.data().guild_configs.get(&guild_id).map_or_else(
        || GuildConfig {
            guild_id: guild_id.get(),
            ..Default::default()
        },
        |entry| entry.clone(),
    )
}

/// Calculates the execution time for an enforcement action
fn calculate_execute_at(action: &EnforcementAction) -> chrono::DateTime<Utc> {
    match action {
        EnforcementAction::Ban { duration }
        | EnforcementAction::Mute { duration }
        | EnforcementAction::VoiceMute { duration }
        | EnforcementAction::VoiceDeafen { duration } => {
            Utc::now() + Duration::seconds(duration.unwrap_or(0) as i64)
        }
        EnforcementAction::Kick { delay } | EnforcementAction::VoiceDisconnect { delay } => {
            Utc::now() + Duration::seconds(delay.unwrap_or(0) as i64)
        }
        EnforcementAction::VoiceChannelHaunt { interval, .. } => {
            Utc::now() + Duration::seconds(interval.unwrap_or(0) as i64)
        }
        EnforcementAction::None => Utc::now(),
    }
}

/// Creates and stores a pending enforcement
async fn create_pending_enforcement(
    ctx: &Context<'_, Data, Error>,
    warning_id: String,
    user_id: u64,
    guild_id: u64,
    action: EnforcementAction,
) -> String {
    let enforcement_id = Uuid::new_v4().to_string();
    let execute_at = calculate_execute_at(&action);

    let pending = PendingEnforcement {
        id: enforcement_id.clone(),
        warning_id,
        user_id,
        guild_id,
        action,
        execute_at: execute_at.to_rfc3339(),
        executed: false,
    };

    ctx.data()
        .pending_enforcements
        .insert(enforcement_id.clone(), pending);

    enforcement_id
}

/// Notifies the enforcement task about a user
async fn notify_enforcement_task(ctx: &Context<'_, Data, Error>, user_id: u64, guild_id: u64) {
    if let Some(tx) = &*ctx.data().enforcement_tx {
        let _ = tx
            .send(EnforcementCheckRequest::CheckUser { user_id, guild_id })
            .await;
    }
}

/// Saves data with appropriate error handling
async fn save_data(ctx: &Context<'_, Data, Error>, error_context: &str) -> Result<(), Error> {
    if let Err(e) = ctx.data().save().await {
        error!("Failed to save data after {}: {}", error_context, e);
        return Err(e);
    }
    Ok(())
}

/// Checks if an enforcement action should be applied immediately
fn is_immediate_action(action: &EnforcementAction) -> bool {
    match action {
        EnforcementAction::Kick { delay } | EnforcementAction::VoiceDisconnect { delay } => {
            delay.is_none() || delay.is_some_and(|d| d == 0)
        }
        EnforcementAction::Mute { duration }
        | EnforcementAction::VoiceMute { duration }
        | EnforcementAction::VoiceDeafen { duration }
        | EnforcementAction::Ban { duration } => {
            duration.is_none() || duration.is_some_and(|d| d == 0)
        }
        EnforcementAction::VoiceChannelHaunt { interval, .. } => {
            interval.is_none() || interval.is_some_and(|d| d == 0)
        }
        EnforcementAction::None => false,
    }
}

/// Creates a pending enforcement and notifies if immediate
async fn create_and_notify_enforcement(
    ctx: &Context<'_, Data, Error>,
    warning_id: String,
    user_id: u64,
    guild_id: u64,
    action: EnforcementAction,
) {
    let _enforcement_id =
        create_pending_enforcement(ctx, warning_id, user_id, guild_id, action.clone()).await;

    if is_immediate_action(&action) {
        notify_enforcement_task(ctx, user_id, guild_id).await;
    }
}

/// Notifies the enforcement task about a specific enforcement
async fn notify_enforcement_task_by_id(ctx: &Context<'_, Data, Error>, enforcement_id: String) {
    if let Some(tx) = &*ctx.data().enforcement_tx {
        let _ = tx
            .send(EnforcementCheckRequest::CheckEnforcement { enforcement_id })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that the ping command is properly defined
    #[test]
    fn test_ping_command_definition() {
        let cmd = ping();
        assert_eq!(cmd.name, "ping");
        assert!(
            cmd.description
                .unwrap_or_else(Default::default)
                .contains("check if the bot is responsive")
        );
        assert!(cmd.guild_only);
    }

    // This test verifies that the ping command can be executed
    #[test]
    fn test_ping_command_can_be_called() {
        // This test just verifies that the ping command exists and can be called
        // We don't actually execute it since that would require a real Discord context
        let cmd = ping();
        assert!(cmd.create_as_slash_command().is_some());
    }
}

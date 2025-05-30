//! Enforcement action handlers
//!
//! This module defines the handlers for different enforcement action types.

use crate::enforcement_new::{
    EnforcementAction, EnforcementActionType, EnforcementError, EnforcementResult,
};
use poise::serenity_prelude::{GuildId, Http, UserId, builder::EditMember};
use rand::Rng;
use serenity::all::{CacheHttp, ChannelId};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Trait for handling enforcement actions
#[async_trait::async_trait]
pub trait ActionHandler: Send + Sync {
    /// Execute the action
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()>;

    /// Reverse the action (if applicable)
    async fn reverse(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()>;
}

/// Registry of action handlers
pub struct ActionHandlerRegistry {
    handlers: HashMap<EnforcementActionType, Box<dyn ActionHandler>>,
}

impl Default for ActionHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionHandlerRegistry {
    /// Create a new registry with all handlers registered
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };

        // Register handlers
        registry.register(EnforcementActionType::None, Box::new(NoopHandler));
        registry.register(EnforcementActionType::Mute, Box::new(MuteHandler));
        registry.register(EnforcementActionType::Ban, Box::new(BanHandler));
        registry.register(EnforcementActionType::Kick, Box::new(KickHandler));
        registry.register(EnforcementActionType::VoiceMute, Box::new(VoiceMuteHandler));
        registry.register(
            EnforcementActionType::VoiceDeafen,
            Box::new(VoiceDeafenHandler),
        );
        registry.register(
            EnforcementActionType::VoiceDisconnect,
            Box::new(VoiceDisconnectHandler),
        );
        registry.register(
            EnforcementActionType::VoiceChannelHaunt,
            Box::new(VoiceChannelHauntHandler),
        );

        registry
    }

    /// Register a handler for an action type
    pub fn register(
        &mut self,
        action_type: EnforcementActionType,
        handler: Box<dyn ActionHandler>,
    ) {
        self.handlers.insert(action_type, handler);
    }

    /// Get a handler for an action type
    #[must_use]
    pub fn get(&self, action_type: EnforcementActionType) -> Option<&dyn ActionHandler> {
        self.handlers.get(&action_type).map(AsRef::as_ref)
    }

    /// Execute an action
    ///
    /// # Errors
    ///
    /// Returns an `EnforcementError` if no handler is registered for the action type.
    pub async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        let action_type = action.get_type();
        if let Some(handler) = self.get(action_type) {
            handler.execute(http, guild_id, user_id, action).await
        } else {
            Err(EnforcementError::ValidationFailed(format!(
                "No handler registered for action type: {action_type}"
            )))
        }
    }

    /// Reverse an action
    ///
    /// # Errors
    ///
    /// Returns an `EnforcementError` if no handler is registered for the action type.
    pub async fn reverse(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        let action_type = action.get_type();
        if let Some(handler) = self.get(action_type) {
            handler.reverse(http, guild_id, user_id, action).await
        } else {
            Err(EnforcementError::ValidationFailed(format!(
                "No handler registered for action type: {action_type}"
            )))
        }
    }
}

/// Helper function to get guild and member
pub async fn get_guild_and_member(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
) -> EnforcementResult<(serenity::all::PartialGuild, serenity::all::Member)> {
    let guild = guild_id.to_partial_guild(http).await.map_err(|e| {
        EnforcementError::GuildOrMemberNotFound(format!("Failed to get guild {guild_id}: {e}"))
    })?;

    let member = guild.member(http, user_id).await.map_err(|e| {
        EnforcementError::GuildOrMemberNotFound(format!(
            "Failed to get member {user_id} in guild {guild_id}: {e}"
        ))
    })?;

    Ok((guild, member))
}

/// Handler for the None action type (no-op)
struct NoopHandler;

#[async_trait::async_trait]
impl ActionHandler for NoopHandler {
    async fn execute(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("No-op action executed for user {user_id} in guild {guild_id}");
        Ok(())
    }

    async fn reverse(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("No-op action reversed for user {user_id} in guild {guild_id}");
        Ok(())
    }
}

/// Handler for the Mute action type
struct MuteHandler;

#[async_trait::async_trait]
impl ActionHandler for MuteHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if let EnforcementAction::Mute(params) = action {
            info!(
                "Muting user {user_id} in guild {guild_id} for {:?} seconds",
                params.duration
            );

            let (_, mut member) = get_guild_and_member(http, guild_id, user_id).await?;

            let timeout_until = chrono::Utc::now()
                + chrono::Duration::seconds(i64::from(params.duration_or_default()));

            member
                .disable_communication_until_datetime(http, timeout_until.into())
                .await
                .map_err(|e| EnforcementError::DiscordApi(Box::new(e)))?;

            info!("Successfully muted user {user_id} until {timeout_until}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected Mute action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        // Discord timeouts are automatically removed when they expire
        info!("Timeout period expired for user {user_id} in guild {guild_id}");
        Ok(())
    }
}

/// Handler for the Ban action type
struct BanHandler;

#[async_trait::async_trait]
impl ActionHandler for BanHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if matches!(action.get_type(), EnforcementActionType::Ban) {
            return Err(EnforcementError::ValidationFailed(
                "Ban action not supported".to_string(),
            ));
        }

        if let EnforcementAction::Ban(params) = action {
            info!(
                "Banning user {user_id} in guild {guild_id} for {:?} seconds",
                params.duration
            );

            let reason = params.reason.clone().unwrap_or_else(|| {
                format!(
                    "Temporary ban from warning system for {} seconds",
                    params.duration_or_default()
                )
            });

            guild_id
                .ban_with_reason(http, user_id, 7, &reason)
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully banned user {user_id}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected Ban action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("Unbanning user {user_id} in guild {guild_id}");

        guild_id
            .unban(http, user_id)
            .await
            .map_err(EnforcementError::from)?;

        info!("Successfully unbanned user {user_id}");
        Ok(())
    }
}

/// Handler for the Kick action type
struct KickHandler;

#[async_trait::async_trait]
impl ActionHandler for KickHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if matches!(action.get_type(), EnforcementActionType::Kick) {
            return Err(EnforcementError::ValidationFailed(
                "Kick action not supported".to_string(),
            ));
        }
        if let EnforcementAction::Kick(params) = action {
            info!("Kicking user {user_id} from guild {guild_id}");

            let (_, member) = get_guild_and_member(http, guild_id, user_id).await?;

            let reason = params
                .reason
                .clone()
                .unwrap_or_else(|| "Kicked by warning system".to_string());

            member
                .kick_with_reason(http, &reason)
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully kicked user {user_id}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected Kick action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        // Kicks can't be reversed
        info!("Kick action doesn't need reversal for user {user_id} in guild {guild_id}");
        Ok(())
    }
}

/// Handler for the `VoiceMute` action type
struct VoiceMuteHandler;

#[async_trait::async_trait]
impl ActionHandler for VoiceMuteHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if let EnforcementAction::VoiceMute(params) = action {
            info!(
                "Voice muting user {user_id} in guild {guild_id} for {:?} seconds",
                params.duration
            );

            let (_, mut member) = get_guild_and_member(http, guild_id, user_id).await?;

            member
                .edit(http, EditMember::new().mute(true))
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully voice muted user {user_id}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected VoiceMute action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("Voice mute period expired for user {user_id} in guild {guild_id}");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            member
                .edit(http, EditMember::new().mute(false))
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully removed voice mute from user {user_id}");
        } else {
            warn!("User {user_id} not found in guild {guild_id} for voice mute reversal");
            // Don't return an error since the user might have left the guild
        }

        Ok(())
    }
}

/// Handler for the `VoiceDeafen` action type
struct VoiceDeafenHandler;

#[async_trait::async_trait]
impl ActionHandler for VoiceDeafenHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if let EnforcementAction::VoiceDeafen(params) = action {
            info!(
                "Voice deafening user {user_id} in guild {guild_id} for {:?} seconds",
                params.duration
            );

            let (_, mut member) = get_guild_and_member(http, guild_id, user_id).await?;

            member
                .edit(http, EditMember::new().deafen(true))
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully voice deafened user {user_id}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected VoiceDeafen action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("Voice deafen period expired for user {user_id} in guild {guild_id}");

        if let Ok((_, mut member)) = get_guild_and_member(http, guild_id, user_id).await {
            member
                .edit(http, EditMember::new().deafen(false))
                .await
                .map_err(EnforcementError::from)?;

            info!("Successfully removed voice deafen from user {user_id}");
        } else {
            warn!("User {user_id} not found in guild {guild_id} for voice deafen reversal");
            // Don't return an error since the user might have left the guild
        }

        Ok(())
    }
}

/// Handler for the `VoiceDisconnect` action type
struct VoiceDisconnectHandler;

#[async_trait::async_trait]
impl ActionHandler for VoiceDisconnectHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        info!("Disconnecting user {user_id} from voice in guild {guild_id}");

        let (_, member) = get_guild_and_member(http, guild_id, user_id).await?;

        member
            .disconnect_from_voice(http)
            .await
            .map_err(EnforcementError::from)?;

        info!("Successfully disconnected user {user_id} from voice");

        Ok(())
    }

    async fn reverse(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        // Voice disconnects can't be reversed
        info!(
            "Voice disconnect action doesn't need reversal for user {user_id} in guild {guild_id}"
        );
        Ok(())
    }
}

/// Handler for the `VoiceChannelHaunt` action type
struct VoiceChannelHauntHandler;

#[async_trait::async_trait]
impl ActionHandler for VoiceChannelHauntHandler {
    async fn execute(
        &self,
        http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        if let EnforcementAction::VoiceChannelHaunt(params) = action {
            info!("Beginning voice channel haunting for user {user_id} in guild {guild_id}");

            // Get guild
            let (guild, _) = get_guild_and_member(http, guild_id, user_id).await?;

            // Find the user's current voice channel
            let current_voice_channel = get_user_voice_channel(http, guild_id, user_id).await?;

            // Get all voice channels in the guild
            let voice_channels = get_guild_voice_channels(http, &guild).await?;

            if voice_channels.is_empty() {
                return Err(EnforcementError::NoVoiceChannels(guild_id.get()));
            }

            // Store the original channel ID if it's not already set
            let original_id = params
                .original_channel_id
                .unwrap_or(current_voice_channel.get());

            // Get parameters
            let teleport_count = params.teleport_count_or_default();
            let delay_seconds = params.interval_or_default();
            let return_to_original = params.return_to_origin_or_default();

            // Execute the haunting
            let http_arc = Arc::new(http);
            let mut failed = false;

            for i in 0..teleport_count {
                if failed {
                    break;
                }

                // Pick a random channel (different from current on first teleport)
                let random_channel =
                    select_random_voice_channel(&voice_channels, i == 0, current_voice_channel);

                // Move the user to the random channel
                failed = !teleport_user(&http_arc.clone(), guild_id, user_id, random_channel).await;

                // Wait before the next teleport if we haven't failed
                if !failed && i < teleport_count - 1 {
                    tokio::time::sleep(tokio::time::Duration::from_secs(delay_seconds.into()))
                        .await;
                }
            }

            // Return the user to their original channel if specified and we haven't failed
            if return_to_original && !failed {
                let original_channel = ChannelId::new(original_id);
                teleport_user(&http_arc, guild_id, user_id, original_channel).await;
            }

            info!("Voice channel haunting completed for user {user_id}");
        } else {
            return Err(EnforcementError::ValidationFailed(
                "Expected VoiceChannelHaunt action".to_string(),
            ));
        }

        Ok(())
    }

    async fn reverse(
        &self,
        _http: &Http,
        guild_id: GuildId,
        user_id: UserId,
        _action: &EnforcementAction,
    ) -> EnforcementResult<()> {
        // Voice channel haunting doesn't need reversal
        info!(
            "Voice channel haunting doesn't need reversal for user {user_id} in guild {guild_id}"
        );
        Ok(())
    }
}

/// Get the current voice channel for a user
async fn get_user_voice_channel(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
) -> EnforcementResult<ChannelId> {
    // Fallback to HTTP if the guild is not in cache
    if let Ok(guild) = guild_id.to_partial_guild(http).await {
        // Get voice channels
        let voice_channels = get_guild_voice_channels(http, &guild).await?;

        for channel_id in voice_channels {
            if let Ok(channel) = channel_id.to_channel(http.http()).await {
                let guild_channel = channel.guild().ok_or_else(|| {
                    EnforcementError::ValidationFailed("Failed to get guild channel".to_string())
                })?;

                let members_result = http.cache().map(|cache| {
                    guild_channel
                        .members(cache)
                        .map(|members| members.iter().any(|member| member.user.id == user_id))
                });

                let is_in_channel = members_result.unwrap_or_else(|| Ok(false)).unwrap_or(false);

                if is_in_channel {
                    // User found in this channel
                    info!("User {user_id} is in voice channel {channel_id}");
                    return Ok(channel_id);
                }
            }
        }

        Err(EnforcementError::NotInVoiceChannel)
    } else {
        Err(EnforcementError::GuildOrMemberNotFound(format!(
            "Failed to get guild {guild_id}"
        )))
    }
}

/// Get all voice channels in a guild
async fn get_guild_voice_channels(
    http: &Http,
    guild: &serenity::all::PartialGuild,
) -> EnforcementResult<Vec<ChannelId>> {
    let channels = guild.channels(http).await.map_err(EnforcementError::from)?;

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

/// Select a random voice channel, optionally ensuring it's different from the current one
fn select_random_voice_channel(
    voice_channels: &[ChannelId],
    must_be_different: bool,
    current_channel: ChannelId,
) -> ChannelId {
    let rng = &mut rand::thread_rng();
    if !must_be_different || voice_channels.len() <= 1 {
        // If we don't need a different channel or there's only one channel, just pick randomly
        let idx = rng.gen_range(0..voice_channels.len());
        voice_channels[idx]
    } else {
        // We need a different channel and have multiple options
        loop {
            let idx = rng.gen_range(0..voice_channels.len());
            let channel = voice_channels[idx];

            if channel != current_channel {
                return channel;
            }
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

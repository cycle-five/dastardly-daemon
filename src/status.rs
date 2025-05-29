use crate::data::{Data, EnforcementState};
use crate::enforcement_new::EnforcementAction;
use ::serenity::all::CacheHttp;
use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use serenity::builder::CreateEmbed;
use serenity::model::id::{ChannelId, GuildId, UserId};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::fmt::{Display, Formatter};
use std::time::SystemTime;
use tracing::info;

/// Structure to track voice channel activity
#[derive(Debug, Clone)]
pub struct VoiceChannelStatus {
    /// Channel ID
    pub channel_id: ChannelId,
    /// Guild ID that this channel belongs to
    pub guild_id: GuildId,
    /// Channel name
    pub name: String,
    /// Set of users currently in this voice channel
    pub users: HashSet<UserId>,
    /// Count of users with active warnings
    pub warned_user_count: usize,
    /// Count of users with active enforcements
    pub enforced_user_count: usize,
    /// Last time this channel was updated
    pub last_updated: SystemTime,
}

impl Default for VoiceChannelStatus {
    fn default() -> Self {
        Self {
            channel_id: ChannelId::new(1),
            guild_id: GuildId::new(1),
            name: String::new(),
            users: HashSet::new(),
            warned_user_count: 0,
            enforced_user_count: 0,
            last_updated: SystemTime::now(),
        }
    }
}

impl Display for VoiceChannelStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Channel ID: {}, Guild ID: {}, Name: {}, Users: {}",
            self.channel_id,
            self.guild_id,
            self.name,
            self.users.len()
        )
    }
}

/// Structure to track a user's voice activity
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UserVoiceStatus {
    /// User ID
    pub user_id: UserId,
    /// Guild ID
    pub guild_id: GuildId,
    /// Current voice channel if any
    pub current_channel: Option<ChannelId>,
    /// User has active warnings
    pub has_warnings: bool,
    /// User has active enforcements
    pub has_enforcements: bool,
    /// Warning level (score)
    pub warning_score: f64,
    /// Time when user joined current voice channel
    pub joined_at: SystemTime,
    /// Time when status was last updated
    pub last_updated: SystemTime,
}

/// Main status tracking struct
#[derive(Debug, Clone)]
pub struct BotStatus {
    /// Map of active voice channels (`ChannelId` -> `VoiceChannelStatus`)
    pub active_voice_channels: DashMap<ChannelId, VoiceChannelStatus>,
    /// Map of users in voice channels ((`UserId`, `GuildId`) -> `UserVoiceStatus`)
    pub users_in_voice: DashMap<(UserId, GuildId), UserVoiceStatus>,
    /// Last time a status check was performed
    pub last_status_check: SystemTime,
}

impl Default for BotStatus {
    fn default() -> Self {
        Self::new()
    }
}

impl BotStatus {
    /// Create a new empty status tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_voice_channels: DashMap::new(),
            users_in_voice: DashMap::new(),
            last_status_check: SystemTime::now(),
        }
    }

    /// Update the status based on current bot data
    pub fn update_from_data(&mut self, data: &Data) {
        // Update the last status check time
        let now = SystemTime::now();
        self.last_status_check = now;

        // First pass: Update user warning and enforcement status
        for user_entry in &self.users_in_voice {
            let key = *user_entry.key();
            let (user_id, guild_id) = key;
            // let guild_id = user_entry.value().guild_id;

            // Check for warnings
            let has_warnings = data.warnings.iter().any(|w| {
                w.value().user_id == user_id.get() && w.value().guild_id == guild_id.get()
            });

            // Check for active enforcements
            let has_enforcements = data.active_enforcements.iter().any(|e| {
                e.value().user_id == user_id.get()
                    && e.value().guild_id == guild_id.get()
                    && e.value().state == EnforcementState::Active
            }) || data.pending_enforcements.iter().any(|e| {
                e.value().user_id == user_id.get()
                    && e.value().guild_id == guild_id.get()
                    && e.value().state == EnforcementState::Pending
            });

            // Calculate warning score
            let warning_score = data.calculate_warning_score(user_id.get(), guild_id.get());

            // Update user status
            if let Some(mut user_status) = self.users_in_voice.get_mut(&key) {
                user_status.has_warnings = has_warnings;
                user_status.has_enforcements = has_enforcements;
                user_status.warning_score = warning_score;
                user_status.last_updated = now;
            }
        }

        // Second pass: Update channel statistics based on user status
        for channel_entry in &self.active_voice_channels {
            let channel_id = *channel_entry.key();
            let guild_id = channel_entry.value().guild_id;
            let mut warned_count = 0;
            let mut enforced_count = 0;

            // Count warned and enforced users in this channel
            for user_id in &channel_entry.value().users {
                let key = (*user_id, guild_id);
                if let Some(user_status) = self.users_in_voice.get(&key) {
                    if user_status.has_warnings {
                        warned_count += 1;
                    }
                    if user_status.has_enforcements {
                        enforced_count += 1;
                    }
                }
            }

            // Update channel status
            if let Some(mut channel_status) = self.active_voice_channels.get_mut(&channel_id) {
                channel_status.warned_user_count = warned_count;
                channel_status.enforced_user_count = enforced_count;
                channel_status.last_updated = now;
            }
        }
    }

    /// Called when a user joins a voice channel
    pub fn user_joined_voice(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        user_id: UserId,
        data: &Data,
    ) {
        let now = SystemTime::now();

        // Get channel name from cache if available
        let channel = data
            .cache
            .guild(guild_id)
            .and_then(|g| g.channels.get(&channel_id).cloned());

        let channel_name = channel
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("Channel {channel_id}"));
        // Update voice channel status
        let mut channel_status = self
            .active_voice_channels
            .get(&channel_id)
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| VoiceChannelStatus {
                channel_id,
                guild_id,
                name: channel_name,
                ..Default::default()
            });

        channel_status.users.insert(user_id);
        channel_status.last_updated = now;
        self.active_voice_channels
            .insert(channel_id, channel_status);

        // Calculate user status
        let has_warnings = data
            .warnings
            .iter()
            .any(|w| w.value().user_id == user_id.get() && w.value().guild_id == guild_id.get());

        let has_enforcements = data.active_enforcements.iter().any(|e| {
            e.value().user_id == user_id.get()
                && e.value().guild_id == guild_id.get()
                && e.value().state == EnforcementState::Active
        }) || data.pending_enforcements.iter().any(|e| {
            e.value().user_id == user_id.get()
                && e.value().guild_id == guild_id.get()
                && e.value().state == EnforcementState::Pending
        });

        let warning_score = data.calculate_warning_score(user_id.get(), guild_id.get());

        // Update user voice status
        let user_status = UserVoiceStatus {
            user_id,
            guild_id,
            current_channel: Some(channel_id),
            has_warnings,
            has_enforcements,
            warning_score,
            joined_at: now,
            last_updated: now,
        };
        self.users_in_voice.insert((user_id, guild_id), user_status);

        // Recalculate channel statistics
        self.recalculate_channel_stats(channel_id);
    }

    /// Called when a user leaves a voice channel
    pub fn user_left_voice(&self, channel_id: ChannelId, user_id: UserId) {
        // Remove user from channel
        if let Some(mut channel_status) = self.active_voice_channels.get_mut(&channel_id) {
            let guild_id = channel_status.guild_id;
            channel_status.users.remove(&user_id);
            channel_status.last_updated = SystemTime::now();

            // If channel is now empty, remove it
            if channel_status.users.is_empty() {
                drop(channel_status); // Drop the reference before removal
                self.active_voice_channels.remove(&channel_id);
            } else {
                // If not empty, recalculate stats
                drop(channel_status); // Drop the reference before recalculation
                self.recalculate_channel_stats(channel_id);
            }
            // Remove or update user status
            self.users_in_voice.remove(&(user_id, guild_id));
        }
    }

    /// Called when a user moves from one voice channel to another
    pub fn user_moved_voice(
        &self,
        guild_id: GuildId,
        old_channel_id: ChannelId,
        new_channel_id: ChannelId,
        user_id: UserId,
        data: &Data,
    ) {
        // Remove from old channel
        self.user_left_voice(old_channel_id, user_id);

        // Add to new channel
        self.user_joined_voice(guild_id, new_channel_id, user_id, data);
    }

    /// Recalculate statistics for a channel based on its current users
    fn recalculate_channel_stats(&self, channel_id: ChannelId) {
        if let Some(mut channel_status) = self.active_voice_channels.get_mut(&channel_id) {
            let mut warned_count = 0;
            let mut enforced_count = 0;
            let guild_id = channel_status.guild_id;

            for user_id in &channel_status.users {
                let key = (*user_id, guild_id);
                if let Some(user_status) = self.users_in_voice.get(&key) {
                    if user_status.value().has_warnings {
                        warned_count += 1;
                    }
                    if user_status.value().has_enforcements {
                        enforced_count += 1;
                    }
                }
            }

            channel_status.warned_user_count = warned_count;
            channel_status.enforced_user_count = enforced_count;
            channel_status.last_updated = SystemTime::now();
        }
    }

    /// Get a list of active voice channels that have users with warnings or enforcements
    #[must_use]
    pub fn get_channels_with_issues(&self) -> Vec<VoiceChannelStatus> {
        self.active_voice_channels
            .iter()
            .filter(|entry| {
                entry.value().warned_user_count > 0 || entry.value().enforced_user_count > 0
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a list of users with active warnings or enforcements who are in voice channels
    #[must_use]
    pub fn get_problematic_users(&self) -> Vec<UserVoiceStatus> {
        self.users_in_voice
            .iter()
            .filter(|entry| entry.value().has_warnings || entry.value().has_enforcements)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get counts of active voice channels and users
    #[must_use]
    pub fn get_active_counts(&self) -> (usize, usize, usize, usize) {
        let active_channels = self.active_voice_channels.len();
        let active_users = self.users_in_voice.len();

        let channels_with_issues = self
            .active_voice_channels
            .iter()
            .filter(|entry| {
                entry.value().warned_user_count > 0 || entry.value().enforced_user_count > 0
            })
            .count();

        let users_with_issues = self
            .users_in_voice
            .iter()
            .filter(|entry| entry.value().has_warnings || entry.value().has_enforcements)
            .count();

        (
            active_channels,
            active_users,
            channels_with_issues,
            users_with_issues,
        )
    }

    /// Initialize status from the current voice states in cache
    pub fn initialize_from_cache(&self, data: &Data) {
        let cache = data.get_cache();

        // Process all guilds
        for guild_id in cache.guilds() {
            if let Some(guild) = cache.guild(guild_id) {
                // Process all voice states in this guild
                for (user_id, voice_state) in &guild.voice_states {
                    if let Some(channel_id) = voice_state.channel_id {
                        // User is in a voice channel
                        self.user_joined_voice(guild_id, channel_id, *user_id, data);
                    }
                }
            }
        }

        info!(
            "Initialized status tracking with {} active voice channels and {} users",
            self.active_voice_channels.len(),
            self.users_in_voice.len()
        );
    }
}

/// Create a pretty-formatted representation of the active voice channels
#[must_use]
pub fn format_active_channels(bot_status: &BotStatus, data: &Data) -> String {
    if bot_status.active_voice_channels.is_empty() {
        return "No active voice channels".to_string();
    }

    let mut result = String::new();
    result.push_str("## Active Voice Channels\n\n");

    let mut channels: Vec<_> = bot_status
        .active_voice_channels
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    // Sort by guild and then by name
    channels.sort_by(|a, b| {
        if a.guild_id == b.guild_id {
            a.name.cmp(&b.name)
        } else {
            a.guild_id.get().cmp(&b.guild_id.get())
        }
    });

    // Group by guild
    let mut current_guild = None;

    for channel in channels {
        // Check if we need a guild header
        if current_guild != Some(channel.guild_id) {
            current_guild = Some(channel.guild_id);

            // Try to get guild name from cache
            let guild_name = data
                .cache
                .guild(channel.guild_id)
                .map_or_else(|| format!("Guild {}", channel.guild_id), |g| g.name.clone());

            let _ = writeln!(result, "\n### {guild_name}");
        }

        // Build status indicators
        let status_indicator = if channel.enforced_user_count > 0 {
            "🔴" // Red circle for enforcements
        } else if channel.warned_user_count > 0 {
            "🟡" // Yellow circle for warnings
        } else {
            "🟢" // Green circle for normal
        };

        // Add channel info
        let _ = writeln!(
            result,
            "- {status_indicator} **{}** (ID: {})",
            channel.name, channel.channel_id
        );

        // Add warning/enforcement counts if any
        if channel.warned_user_count > 0 || channel.enforced_user_count > 0 {
            let _ = writeln!(
                result,
                " ({} warned, {} enforced)",
                channel.warned_user_count, channel.enforced_user_count,
            );
        }

        result.push('\n');
        let _ = writeln!(result);
    }

    result
}

/// Create a pretty-formatted representation of users with warnings or enforcements
#[must_use]
pub async fn format_problematic_users(
    bot_status: &BotStatus,
    data: &Data,
    cache_http: &impl CacheHttp,
) -> String {
    let problematic_users = bot_status.get_problematic_users();

    if problematic_users.is_empty() {
        return "No users with active warnings or enforcements in voice channels".to_string();
    }

    let mut result = String::new();
    result.push_str("## Users with Warnings or Enforcements\n\n");

    // Sort by warning score (highest first)
    let mut users = problematic_users;
    users.sort_by(|a, b| {
        b.warning_score
            .partial_cmp(&a.warning_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for user in users {
        // Try to get user name from cache
        let user_name = data
            .cache
            .user(user.user_id)
            .map_or_else(|| format!("User {}", user.user_id), |u| u.name.clone());

        // Get channel name if user is in one
        let channel_info = if let Some(channel_id) = user.current_channel {
            if let Ok(chanel) = channel_id.to_channel(cache_http).await {
                if let Some(guild_chanel) = chanel.guild() {
                    let name = guild_chanel.name;
                    format!(" in **{name}**")
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Status indicator
        let status = if user.has_enforcements {
            "🔴 **ENFORCED**"
        } else if user.has_warnings {
            "🟡 **WARNED**"
        } else {
            "⚪"
        };

        // Add user info with score
        result.push_str(&format!(
            "- {status} **{user_name}** (Score: {:.2}){channel_info}",
            user.warning_score
        ));

        result.push('\n');
    }

    result
}

/// Create a pretty-formatted representation of the pending and active enforcements
#[must_use]
pub fn format_enforcement_status(data: &Data) -> String {
    // Get pending and active enforcements
    let pending: Vec<_> = data
        .pending_enforcements
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    let active: Vec<_> = data
        .active_enforcements
        .iter()
        .map(|entry| entry.value().clone())
        .collect();

    if pending.is_empty() && active.is_empty() {
        return "No pending or active enforcements".to_string();
    }

    let mut result = String::new();
    result.push_str("## Active Enforcement Status\n\n");

    // Process pending enforcements
    if !pending.is_empty() {
        result.push_str("### Pending Enforcements\n");

        for enforcement in pending {
            let user_id = enforcement.user_id;
            let user_name = data
                .cache
                .user(UserId::new(user_id))
                .map_or_else(|| format!("User {user_id}"), |u| u.name.clone());

            // Format the action in a more readable way
            let action_str = format_enforcement_action(&enforcement.action);

            let _ = writeln!(
                result,
                "- **{user_name}**: {action_str} - Scheduled at {}",
                enforcement.execute_at
            );
        }

        result.push('\n');
    }

    // Process active enforcements
    if !active.is_empty() {
        let _ = writeln!(result, "### Active Enforcements\n");

        for enforcement in active {
            let user_id = enforcement.user_id;
            let user_name = data
                .cache
                .user(UserId::new(user_id))
                .map_or_else(|| format!("User {user_id}"),|u| u.name.clone());

            // Format the action in a more readable way
            let action_str = format_enforcement_action(&enforcement.action);

            // Add reversal time if set
            let reversal_info = if let Some(reverse_at) = &enforcement.reverse_at {
                format!(" - Will be reversed at {reverse_at}")
            } else {
                String::new()
            };

            let _ = writeln!(result, "- **{user_name}**: {action_str}{reversal_info}");
        }
    }

    result
}

impl Display for EnforcementAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format_enforcement_action(self))
    }
}

/// Format an enforcement action in a human-readable way
fn format_enforcement_action(action: &EnforcementAction) -> String {
    match action {
        EnforcementAction::Mute(params) => {
            format!("Muted for {} seconds", params.duration_or_default())
        }
        EnforcementAction::Ban(params) => {
            format!("Banned for {} seconds", params.duration_or_default())
        }
        EnforcementAction::Kick(params) => {
            if params.has_duration() {
                let delay = params.duration_or_default();
                if delay > 0 {
                    format!("Will be kicked in {delay} seconds")
                } else {
                    "Kicked".to_string()
                }
            } else {
                "Kicked".to_string()
            }
        }
        EnforcementAction::VoiceMute(params) => {
            format!("Voice muted for {} seconds", params.duration_or_default())
        }
        EnforcementAction::VoiceDeafen(params) => {
            format!(
                "Voice deafened for {} seconds",
                params.duration_or_default()
            )
        }
        EnforcementAction::VoiceDisconnect(params) => {
            if params.has_duration() {
                let delay = params.duration_or_default();
                if delay > 0 {
                    format!("Will be disconnected from voice in {delay} seconds")
                } else {
                    "Disconnected from voice".to_string()
                }
            } else {
                "Disconnected from voice".to_string()
            }
        }
        EnforcementAction::VoiceChannelHaunt(params) => {
            format!(
                "Voice haunting: {} teleports every {} seconds{}",
                params.teleport_count_or_default(),
                params.interval_or_default(),
                if params.return_to_origin_or_default() {
                    " (will return to origin)"
                } else {
                    ""
                }
            )
        }
        EnforcementAction::None => "No action".to_string(),
    }
}

/// Format a complete status report of the bot
#[must_use]
pub async fn format_complete_status(
    bot_status: &BotStatus,
    data: &Data,
    cache_http: &impl CacheHttp,
) -> String {
    let (total_channels, total_users, issue_channels, issue_users) = bot_status.get_active_counts();

    let mut result = String::new();

    // System status summary
    let _ = writeln!(result, "# Dastardly Daemon Status Report\n");

    let _ = writeln!(
        result,
        "**Active Voice Channels**: {total_channels} (with {issue_channels} having warned/enforced users)",
    );

    let _ = writeln!(
        result,
        "**Users in Voice**: {total_users} (with {issue_users} having warnings/enforcements)",
    );

    let channels_with_issues = bot_status.get_channels_with_issues();
    let num_channels_with_issues = channels_with_issues.len();
    let num_users_with_issues = bot_status
        .users_in_voice
        .iter()
        .filter(|entry| entry.value().has_warnings || entry.value().has_enforcements)
        .count();

    // Add problematic channels/users
    if num_channels_with_issues > 0 {
        let _ = writeln!(
            result,
            "**Problematic Channels**: {num_channels_with_issues} channels with warned/enforced users"
        );
    }
    if num_users_with_issues > 0 {
        let _ = writeln!(
            result,
            "**Problematic Users**: {num_users_with_issues} users with warnings/enforcements"
        );
    }

    // Add pending/active enforcement counts
    let pending_count = data.pending_enforcements.len();
    let active_count = data.active_enforcements.len();

    let _ = writeln!(
        result,
        "**Enforcements**: {pending_count} pending, {active_count} active"
    );
    let _ = writeln!(
        result,
        "**Last Status Update**: {}\n",
        format_system_time(bot_status.last_status_check)
    );

    // Add detailed sections
    if issue_channels > 0 {
        let _ = write!(result, "{}", format_active_channels(bot_status, data));
    }

    if issue_users > 0 {
        let _ = writeln!(
            result,
            "{}",
            &format_problematic_users(bot_status, data, cache_http).await
        );
    }

    if pending_count > 0 || active_count > 0 {
        let _ = write!(result, "{}", format_enforcement_status(data));
    }

    result
}

/// Helper to format a `SystemTime` in a human-readable format
fn format_system_time(time: SystemTime) -> String {
    let now = SystemTime::now();

    if let Ok(duration) = now.duration_since(time) {
        if duration.as_secs() < 60 {
            "just now".to_string()
        } else if duration.as_secs() < 3600 {
            format!("{} minutes ago", duration.as_secs() / 60)
        } else if duration.as_secs() < 86400 {
            format!("{} hours ago", duration.as_secs() / 3600)
        } else {
            format!("{} days ago", duration.as_secs() / 86400)
        }
    } else {
        "unknown time".to_string()
    }
}

/// Create an embed for displaying bot status
pub fn _create_status_embed(bot_status: &BotStatus, data: &Data) -> CreateEmbed {
    let (total_channels, total_users, issue_channels, issue_users) = bot_status.get_active_counts();
    let pending_count = data.pending_enforcements.len();
    let active_count = data.active_enforcements.len();

    let mut embed = CreateEmbed::new()
        .title("Daemon Status")
        .description("Current state of the Dastardly Daemon")
        .field("Voice Channels", format!("{total_channels} active"), true)
        .field("Users in Voice", format!("{total_users} active"), true)
        .field(
            "Enforcements",
            format!("{pending_count} pending, {active_count} active"),
            true,
        )
        .timestamp(serenity::Timestamp::now());

    // Add information about problematic channels/users if any
    if issue_channels > 0 {
        embed = embed.field(
            "Problematic Channels",
            format!("{issue_channels} channels with warned/enforced users"),
            false,
        );
    }

    if issue_users > 0 {
        // Add details about top 5 problematic users
        let mut top_users = bot_status.get_problematic_users();
        top_users.sort_by(|a, b| {
            b.warning_score
                .partial_cmp(&a.warning_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut user_list = String::new();
        for user in top_users.iter().take(5) {
            let user_name = data
                .cache
                .user(user.user_id)
                .map_or_else(|| format!("User {}", user.user_id), |u| u.name.clone());

            let status = if user.has_enforcements {
                "🔴"
            } else if user.has_warnings {
                "🟡"
            } else {
                "⚪"
            };

            writeln!(
                user_list,
                "{status} **{user_name}** - Score: {:.2}",
                user.warning_score
            )
            .unwrap();
        }

        if !user_list.is_empty() {
            embed = embed.field("Top Problematic Users", user_list, false);
        }
    }

    embed
}

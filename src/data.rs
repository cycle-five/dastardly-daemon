use std::{
    default::Default,
    fmt::{Display, Formatter},
    ops::{Deref, DerefMut},
    sync::Arc,
};

use crate::enforcement::EnforcementCheckRequest;
use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};
use serenity::prelude::TypeMapKey;
use tokio::sync::mpsc::Sender;

// Constants for the scoring algorithm
const DECAY_RATE: f64 = 0.05; // Higher values mean faster decay
const MOD_DIVERSITY_BONUS: f64 = 0.5; // Bonus for different mods reporting

/// Guild configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildConfig {
    // The ID of the guild
    pub guild_id: u64,
    // For example if you're doing a music bot, this could be the ID of the channel
    // where the bot should send music messages.
    pub music_channel_id: Option<u64>,
    // Default notification method for warnings
    pub default_notification_method: NotificationMethod,
    // Default enforcement action for warnings
    pub default_enforcement: Option<EnforcementAction>,
    // Channel for public enforcement logs
    pub enforcement_log_channel_id: Option<u64>,
    // Chaos factor for randomness in enforcement decisions (0.0-1.0)
    pub chaos_factor: f32,
    // Warning threshold for the weighted warning system
    pub warning_threshold: f64,
}

impl Default for GuildConfig {
    fn default() -> Self {
        Self {
            guild_id: 0,
            music_channel_id: None,
            default_notification_method: NotificationMethod::PublicWithMention,
            default_enforcement: None,
            enforcement_log_channel_id: None,
            chaos_factor: 0.3,
            warning_threshold: 2.0,
        }
    }
}

/// Notification method for warnings
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum NotificationMethod {
    #[default]
    DirectMessage,
    PublicWithMention,
}

/// Enforcement actions that can be taken as part of a warning
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    #[default]
    None,
    Mute {
        duration: Option<u64>,
    },
    Ban {
        duration: Option<u64>,
    },
    Kick {
        delay: Option<u64>,
    },
    // Voice channel specific actions
    VoiceMute {
        duration: Option<u64>,
    },
    VoiceDeafen {
        duration: Option<u64>,
    },
    VoiceDisconnect {
        delay: Option<u64>,
    },
    // Daemon specialized punishments
    VoiceChannelHaunt {
        /// Number of times to teleport the user between channels
        teleport_count: Option<u64>,
        /// Seconds between each teleport
        interval: Option<u64>,
        /// Whether to eventually return the user to their original channel
        return_to_origin: Option<bool>,
        /// Original voice channel ID to potentially return to
        original_channel_id: Option<u64>,
    },
}

/// Represents a warning issued to a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub id: String,
    pub user_id: u64,
    pub issuer_id: u64,
    pub guild_id: u64,
    pub reason: String,
    pub timestamp: String,
    pub notification_method: NotificationMethod,
    pub enforcement: Option<EnforcementAction>,
}

impl Display for Warning {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Warning ID: {}. User ID: {}. Issuer ID: {}. Guild ID: {}. Reason: {}. Timestamp: {}.",
            self.id, self.user_id, self.issuer_id, self.guild_id, self.reason, self.timestamp
        ))
    }
}

/// Represents the context of a warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarningContext {
    pub user_name: String,
    pub num_warn: u64,
    pub voice_warnings: Vec<Warning>,
    pub warning_score: f64,
    pub warning_threshold: f64,
    pub mod_name: String,
}

impl Display for WarningContext {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "User: {}. Total warnings: {}. Voice warnings: {:?}. Current score: {:.2}. Threshold: {:.1}. Moderator: {}.",
            self.user_name,
            self.num_warn,
            self.voice_warnings,
            self.warning_score,
            self.warning_threshold,
            self.mod_name
        ))
    }
}

/// Represents a pending enforcement action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEnforcement {
    pub id: String,
    pub warning_id: String,
    pub user_id: u64,
    pub guild_id: u64,
    pub action: EnforcementAction,
    pub execute_at: String,
    pub executed: bool,
}

/// Tracks warning state for a user, used for the weighted warning system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserWarningState {
    pub user_id: u64,
    pub guild_id: u64,
    pub warning_timestamps: Vec<String>, // Stored as RFC3339 strings
    pub warning_reasons: Vec<String>,
    pub mod_issuers: Vec<u64>,
    pub pending_enforcement: Option<EnforcementAction>,
    pub last_updated: String, // RFC3339 timestamp
}

/// Centralized data structure for the bot
#[derive(Clone)]
pub struct Data(pub Arc<DataInner>);

// Implement TypeMapKey for Data to allow storing it in Serenity's data map
impl TypeMapKey for Data {
    type Value = Data;
}

impl Default for Data {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Data")
            .field("guild_configs", &self.guild_configs)
            .field("cache", &self.cache)
            .field("warnings", &self.warnings)
            .field("pending_enforcements", &self.pending_enforcements)
            .field("enforcement_tx", &self.enforcement_tx)
            .finish()
    }
}

impl Data {
    /// Get the guild configuration for a specific guild
    #[must_use]
    pub fn get_guild_config(&self, guild_id: serenity::GuildId) -> Option<GuildConfig> {
        self.0
            .guild_configs
            .get(&guild_id)
            .map(|entry| entry.value().clone())
    }

    /// Get the cache
    #[must_use]
    pub fn get_cache(&self) -> Arc<serenity::Cache> {
        Arc::clone(&self.0.cache)
    }
}

impl Deref for Data {
    type Target = DataInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Data {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl Data {
    /// Create a new Data instance
    #[must_use]
    pub fn new() -> Self {
        Self(DataInner::new().into())
    }

    /// Set the enforcement task sender
    pub fn set_enforcement_tx(&mut self, tx: Sender<EnforcementCheckRequest>) {
        Arc::make_mut(&mut self.0).enforcement_tx = Arc::new(Some(tx));
    }

    /// Load data from YAML file
    pub async fn load() -> Self {
        Self(Arc::new(DataInner::load().await))
    }

    /// Save data to YAML file
    /// # Errors
    /// This function will return an error if:
    /// - The config directory cannot be created
    /// - The guild configurations cannot be serialized to YAML
    /// - The YAML data cannot be written to the config file
    pub async fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.0.save().await
    }

    /// Get the enforcement task sender
    #[must_use]
    pub fn get_warnings(&self) -> Vec<Warning> {
        self.0
            .warnings
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get the pending enforcement actions
    #[must_use]
    pub fn get_pending_enforcements(&self) -> Vec<PendingEnforcement> {
        self.0
            .pending_enforcements
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a specific warning by ID
    #[must_use]
    pub fn get_warning(&self, id: &str) -> Option<Warning> {
        self.0.warnings.get(id).map(|entry| entry.value().clone())
    }

    /// Get a user's warning state or create a new one if it doesn't exist
    #[must_use]
    pub fn get_or_create_user_warning_state(
        &self,
        user_id: u64,
        guild_id: u64,
    ) -> UserWarningState {
        let key = format!("{user_id}:{guild_id}");
        if let Some(state) = self.0.user_warning_states.get(&key) {
            state.value().clone()
        } else {
            UserWarningState {
                user_id,
                guild_id,
                warning_timestamps: Vec::new(),
                warning_reasons: Vec::new(),
                mod_issuers: Vec::new(),
                pending_enforcement: None,
                last_updated: chrono::Utc::now().to_rfc3339(),
            }
        }
    }

    /// Add a warning to a user's warning state
    #[must_use]
    pub fn add_to_user_warning_state(
        &self,
        user_id: u64,
        guild_id: u64,
        reason: String,
        issuer_id: u64,
    ) -> UserWarningState {
        let key = format!("{user_id}:{guild_id}");
        let timestamp = chrono::Utc::now().to_rfc3339();

        let mut state = self.get_or_create_user_warning_state(user_id, guild_id);
        state.warning_timestamps.push(timestamp.clone());
        state.warning_reasons.push(reason);
        state.mod_issuers.push(issuer_id);
        state.last_updated = timestamp;

        self.0.user_warning_states.insert(key, state.clone());
        state
    }

    /// Calculate a weighted warning score for a user based on recency and mod diversity
    /// Returns a score from 0.0 to infinity where higher scores mean more warnings
    #[must_use]
    pub fn calculate_warning_score(&self, user_id: u64, guild_id: u64) -> f64 {
        let state = self.get_or_create_user_warning_state(user_id, guild_id);
        if state.warning_timestamps.is_empty() {
            return 0.0;
        }

        let now = chrono::Utc::now();
        let mut total_score = 0.0;
        let mut unique_mods = std::collections::HashSet::new();

        // Calculate score for each warning based on recency
        for (i, timestamp_str) in state.warning_timestamps.iter().enumerate() {
            if let Ok(timestamp) = chrono::DateTime::parse_from_rfc3339(timestamp_str) {
                let age_hours =
                    (now - timestamp.with_timezone(&chrono::Utc)).num_seconds() as f64 / 3600.0;
                let weight = (-DECAY_RATE * age_hours).exp(); // Exponential decay based on age
                total_score += weight;

                // Track unique mods who issued warnings
                if i < state.mod_issuers.len() {
                    unique_mods.insert(state.mod_issuers[i]);
                }
            }
        }

        // Apply a bonus if multiple mods issued warnings (more credible reports)
        if unique_mods.len() > 1 {
            total_score += MOD_DIVERSITY_BONUS * (unique_mods.len() as f64 - 1.0);
        }

        total_score
    }
}

/// Main centralized data structure for the bot
#[derive(Clone)]
pub struct DataInner {
    // Map of guild_id -> guild configuration
    pub guild_configs: DashMap<serenity::GuildId, GuildConfig>,
    // Cache from the bot's context
    pub cache: Arc<serenity::Cache>,
    // Map of warning_id -> warning
    pub warnings: DashMap<String, Warning>,
    // Map of enforcement_id -> pending enforcement
    pub pending_enforcements: DashMap<String, PendingEnforcement>,
    // Map of user_id+guild_id -> user warning state
    pub user_warning_states: DashMap<String, UserWarningState>,
    // Channel to send enforcement check requests
    pub enforcement_tx: Arc<Option<Sender<EnforcementCheckRequest>>>,
}

impl Default for DataInner {
    fn default() -> Self {
        Self::new()
    }
}

impl DataInner {
    // Create a new Data instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            guild_configs: DashMap::new(),
            cache: Arc::new(serenity::Cache::default()),
            warnings: DashMap::new(),
            pending_enforcements: DashMap::new(),
            user_warning_states: DashMap::new(),
            enforcement_tx: Arc::new(None),
        }
    }

    /// Load data from YAML file
    ///
    /// This method loads guild configurations from a YAML file.
    /// If the file doesn't exist, it returns a new empty Data instance.
    pub async fn load() -> Self {
        const CONFIG_FILE: &str = "data/bot_config.yaml";
        const WARNINGS_FILE: &str = "data/warnings.yaml";
        const ENFORCEMENTS_FILE: &str = "data/enforcements.yaml";
        const WARNING_STATES_FILE: &str = "data/warning_states.yaml";

        // Create a new empty Data instance
        let data = Self::new();

        // Check if the config file exists
        if let Ok(file_content) = tokio::fs::read_to_string(CONFIG_FILE).await {
            // Try to deserialize the file content
            if let Ok(configs) = serde_yaml::from_str::<Vec<GuildConfig>>(&file_content) {
                // Add each guild config to the map
                for config in configs {
                    let guild_id = serenity::GuildId::new(config.guild_id);
                    data.guild_configs.insert(guild_id, config);
                }
            }
        }

        // Load warnings
        if let Ok(file_content) = tokio::fs::read_to_string(WARNINGS_FILE).await {
            if let Ok(warnings) = serde_yaml::from_str::<Vec<Warning>>(&file_content) {
                for warning in warnings {
                    data.warnings.insert(warning.id.clone(), warning);
                }
            }
        }

        // Load pending enforcements
        if let Ok(file_content) = tokio::fs::read_to_string(ENFORCEMENTS_FILE).await {
            if let Ok(enforcements) = serde_yaml::from_str::<Vec<PendingEnforcement>>(&file_content)
            {
                for enforcement in enforcements {
                    data.pending_enforcements
                        .insert(enforcement.id.clone(), enforcement);
                }
            }
        }

        // Load user warning states
        if let Ok(file_content) = tokio::fs::read_to_string(WARNING_STATES_FILE).await {
            if let Ok(states) = serde_yaml::from_str::<Vec<UserWarningState>>(&file_content) {
                for state in states {
                    let key = format!("{}:{}", state.user_id, state.guild_id);
                    data.user_warning_states.insert(key, state);
                }
            }
        }

        data
    }

    /// Save data to YAML file
    ///
    /// This method saves all guild configurations to a YAML file.
    /// It creates the config directory if it doesn't exist.
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The config directory cannot be created
    /// - The guild configurations cannot be serialized to YAML
    /// - The YAML data cannot be written to the config file
    pub async fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        const CONFIG_DIR: &str = "config";
        const CONFIG_FILE: &str = "data/bot_config.yaml";
        const WARNINGS_FILE: &str = "data/warnings.yaml";
        const ENFORCEMENTS_FILE: &str = "data/enforcements.yaml";
        const WARNING_STATES_FILE: &str = "data/warning_states.yaml";

        // Create the config directory if it doesn't exist
        if !std::path::Path::new(CONFIG_DIR).exists() {
            tokio::fs::create_dir_all(CONFIG_DIR).await?;
        }

        // Collect all guild configs into a Vec for serialization
        let configs: Vec<GuildConfig> = self
            .guild_configs
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        // Serialize the configs to YAML
        let yaml = serde_yaml::to_string(&configs)?;

        // Write the YAML to the config file
        tokio::fs::write(CONFIG_FILE, yaml).await?;

        // Save warnings
        let warnings: Vec<Warning> = self
            .warnings
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        let warnings_yaml = serde_yaml::to_string(&warnings)?;
        tokio::fs::write(WARNINGS_FILE, warnings_yaml).await?;

        // Save pending enforcements
        let enforcements: Vec<PendingEnforcement> = self
            .pending_enforcements
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        let enforcements_yaml = serde_yaml::to_string(&enforcements)?;
        tokio::fs::write(ENFORCEMENTS_FILE, enforcements_yaml).await?;

        // Save user warning states
        let warning_states: Vec<UserWarningState> = self
            .user_warning_states
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        let warning_states_yaml = serde_yaml::to_string(&warning_states)?;
        tokio::fs::write(WARNING_STATES_FILE, warning_states_yaml).await?;

        Ok(())
    }
}

/// Tests for the data module
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_new() {
        let data = Data::new();
        assert_eq!(data.guild_configs.len(), 0);
        assert!(data.cache.guilds().is_empty());
        assert_eq!(data.warnings.len(), 0);
        assert_eq!(data.pending_enforcements.len(), 0);
    }

    #[test]
    fn test_guild_config_default() {
        let config = GuildConfig::default();
        assert_eq!(config.guild_id, 0);
        assert!(config.music_channel_id.is_none());
        assert!(matches!(
            config.default_notification_method,
            NotificationMethod::PublicWithMention
        ));
        assert!(config.default_enforcement.is_none());
    }

    #[test]
    fn test_data_debug_impl() {
        let data = Data::new();
        let debug_output = format!("{:?}", data);
        assert!(debug_output.contains("Data"));
        assert!(debug_output.contains("guild_configs"));
        assert!(debug_output.contains("cache"));
        assert!(debug_output.contains("warnings"));
        assert!(debug_output.contains("pending_enforcements"));
    }

    #[test]
    fn test_guild_config_serialization() {
        let config = GuildConfig {
            guild_id: 12345,
            music_channel_id: Some(67890),
            default_notification_method: NotificationMethod::DirectMessage,
            default_enforcement: Some(EnforcementAction::Mute {
                duration: Some(3600),
            }),
            enforcement_log_channel_id: Some(54321),
            ..Default::default()
        };

        // Test serialization
        let serialized = serde_yaml::to_string(&config).expect("Failed to serialize");
        assert!(serialized.contains("guild_id: 12345"));
        assert!(serialized.contains("music_channel_id: 67890"));
        assert!(serialized.contains("default_notification_method: DirectMessage"));
        assert!(serialized.contains("default_enforcement:"));

        // Test deserialization
        let deserialized: GuildConfig =
            serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.guild_id, 12345);
        assert_eq!(deserialized.music_channel_id, Some(67890));
        assert!(matches!(
            deserialized.default_notification_method,
            NotificationMethod::DirectMessage
        ));
        if let Some(EnforcementAction::Mute { duration }) = deserialized.default_enforcement {
            assert_eq!(duration, Some(3600));
        } else {
            panic!("Expected Mute enforcement");
        }
    }

    #[test]
    fn test_warning_serialization() {
        let warning = Warning {
            id: "test-id".to_string(),
            user_id: 12345,
            issuer_id: 67890,
            guild_id: 11111,
            reason: "Test warning".to_string(),
            timestamp: "2023-01-01T00:00:00Z".to_string(),
            notification_method: NotificationMethod::PublicWithMention,
            enforcement: Some(EnforcementAction::Kick { delay: Some(86400) }),
        };

        let serialized = serde_yaml::to_string(&warning).expect("Failed to serialize");
        assert!(serialized.contains("id: test-id"));
        assert!(serialized.contains("user_id: 12345"));
        assert!(serialized.contains("notification_method: PublicWithMention"));
        assert!(serialized.contains("enforcement:"));
        assert!(serialized.contains("Kick"));

        let deserialized: Warning =
            serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.user_id, 12345);
        assert!(matches!(
            deserialized.notification_method,
            NotificationMethod::PublicWithMention
        ));
        if let Some(EnforcementAction::Kick { delay }) = deserialized.enforcement {
            assert_eq!(delay, Some(86400));
        } else {
            panic!("Expected Kick enforcement");
        }
    }

    #[test]
    fn test_pending_enforcement_serialization() {
        let enforcement = PendingEnforcement {
            id: "enf-id".to_string(),
            warning_id: "warn-id".to_string(),
            user_id: 12345,
            guild_id: 11111,
            action: EnforcementAction::Ban {
                duration: Some(604800),
            },
            execute_at: "2023-01-02T00:00:00Z".to_string(),
            executed: false,
        };

        let serialized = serde_yaml::to_string(&enforcement).expect("Failed to serialize");
        assert!(serialized.contains("id: enf-id"));
        assert!(serialized.contains("warning_id: warn-id"));
        assert!(serialized.contains("executed: false"));
        assert!(serialized.contains("Ban"));

        let deserialized: PendingEnforcement =
            serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.id, "enf-id");
        assert_eq!(deserialized.warning_id, "warn-id");
        assert!(!deserialized.executed);
        if let EnforcementAction::Ban { duration } = deserialized.action {
            assert_eq!(duration, Some(604800));
        } else {
            panic!("Expected Ban enforcement");
        }
    }
}

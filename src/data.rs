use std::sync::Arc;

use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use serenity::prelude::TypeMapKey;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use crate::enforcement::EnforcementCheckRequest;

/// Guild configuration structure.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
}

/// Notification method for warnings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationMethod {
    DirectMessage,
    PublicWithMention,
}

impl Default for NotificationMethod {
    fn default() -> Self {
        NotificationMethod::DirectMessage
    }
}

/// Enforcement actions that can be taken as part of a warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementAction {
    None,
    Mute { duration: u64 },
    Ban { duration: u64 },
    DelayedKick { delay: u64 },
}

impl Default for EnforcementAction {
    fn default() -> Self {
        EnforcementAction::None
    }
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

/// Main centralized data structure for the bot
#[derive(Clone)]
pub struct Data {
    // Map of guild_id -> guild configuration
    pub guild_configs: DashMap<serenity::GuildId, GuildConfig>,
    // Cache from the bot's context
    pub cache: Arc<serenity::Cache>,
    // Map of warning_id -> warning
    pub warnings: DashMap<String, Warning>,
    // Map of enforcement_id -> pending enforcement
    pub pending_enforcements: DashMap<String, PendingEnforcement>,
    // Channel to send enforcement check requests
    pub enforcement_tx: Option<Sender<EnforcementCheckRequest>>,
}

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
            .finish()
    }
}

impl Data {
    // Create a new Data instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            guild_configs: DashMap::new(),
            cache: Arc::new(serenity::Cache::default()),
            warnings: DashMap::new(),
            pending_enforcements: DashMap::new(),
            enforcement_tx: None,
        }
    }
    
    /// Set the enforcement task sender
    pub fn set_enforcement_tx(&mut self, tx: Sender<EnforcementCheckRequest>) {
        self.enforcement_tx = Some(tx);
    }

    /// Load data from YAML file
    ///
    /// This method loads guild configurations from a YAML file.
    /// If the file doesn't exist, it returns a new empty Data instance.
    pub async fn load() -> Self {
        const CONFIG_FILE: &str = "config/bot_config.yaml";
        const WARNINGS_FILE: &str = "config/warnings.yaml";
        const ENFORCEMENTS_FILE: &str = "config/enforcements.yaml";

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
        const CONFIG_FILE: &str = "config/bot_config.yaml";
        const WARNINGS_FILE: &str = "config/warnings.yaml";
        const ENFORCEMENTS_FILE: &str = "config/enforcements.yaml";

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
            NotificationMethod::DirectMessage
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
            default_enforcement: Some(EnforcementAction::Mute { duration: 3600 }),
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
            assert_eq!(duration, 3600);
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
            enforcement: Some(EnforcementAction::DelayedKick { delay: 86400 }),
        };

        let serialized = serde_yaml::to_string(&warning).expect("Failed to serialize");
        assert!(serialized.contains("id: test-id"));
        assert!(serialized.contains("user_id: 12345"));
        assert!(serialized.contains("notification_method: PublicWithMention"));
        assert!(serialized.contains("enforcement:"));
        assert!(serialized.contains("DelayedKick:"));

        let deserialized: Warning =
            serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.id, "test-id");
        assert_eq!(deserialized.user_id, 12345);
        assert!(matches!(
            deserialized.notification_method,
            NotificationMethod::PublicWithMention
        ));
        if let Some(EnforcementAction::DelayedKick { delay }) = deserialized.enforcement {
            assert_eq!(delay, 86400);
        } else {
            panic!("Expected DelayedKick enforcement");
        }
    }

    #[test]
    fn test_pending_enforcement_serialization() {
        let enforcement = PendingEnforcement {
            id: "enf-id".to_string(),
            warning_id: "warn-id".to_string(),
            user_id: 12345,
            guild_id: 11111,
            action: EnforcementAction::Ban { duration: 604800 },
            execute_at: "2023-01-02T00:00:00Z".to_string(),
            executed: false,
        };

        let serialized = serde_yaml::to_string(&enforcement).expect("Failed to serialize");
        assert!(serialized.contains("id: enf-id"));
        assert!(serialized.contains("warning_id: warn-id"));
        assert!(serialized.contains("executed: false"));
        assert!(serialized.contains("Ban:"));

        let deserialized: PendingEnforcement =
            serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.id, "enf-id");
        assert_eq!(deserialized.warning_id, "warn-id");
        assert!(!deserialized.executed);
        if let EnforcementAction::Ban { duration } = deserialized.action {
            assert_eq!(duration, 604800);
        } else {
            panic!("Expected Ban enforcement");
        }
    }
}

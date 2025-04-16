use std::sync::Arc;

use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};

/// Guild configuration structure.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GuildConfig {
    // The ID of the guild
    pub guild_id: u64,
    // For example if you're doing a music bot, this could be the ID of the channel
    // where the bot should send music messages.
    pub music_channel_id: Option<u64>,
}

/// Main centrailized data structure for the bot. Should it use the `InnerData` idiom?
#[derive(Clone)]
pub struct Data {
    // Map of guild_id -> guild configuration, you'll need one of these for anything more
    // than the most trivial commands.
    pub guild_configs: dashmap::DashMap<serenity::GuildId, GuildConfig>,
    // Cache from the bot's context, you'll probably need this for some commands
    pub cache: Arc<serenity::Cache>,
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
            .finish()
    }
}

impl Data {
    // Create a new Data instance
    #[must_use]
    pub fn new() -> Self {
        Self {
            guild_configs: dashmap::DashMap::new(),
            cache: Arc::new(serenity::Cache::default()),
        }
    }

    /// Load data from YAML file
    ///
    /// This method loads guild configurations from a YAML file.
    /// If the file doesn't exist, it returns a new empty Data instance.
    pub async fn load() -> Self {
        const CONFIG_FILE: &str = "config/bot_config.yaml";
        
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
        
        // Create the config directory if it doesn't exist
        if !std::path::Path::new(CONFIG_DIR).exists() {
            tokio::fs::create_dir_all(CONFIG_DIR).await?;
        }
        
        // Collect all guild configs into a Vec for serialization
        let configs: Vec<GuildConfig> = self.guild_configs
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
        
        // Serialize the configs to YAML
        let yaml = serde_yaml::to_string(&configs)?;
        
        // Write the YAML to the config file
        tokio::fs::write(CONFIG_FILE, yaml).await?;
        
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
    }

    #[test]
    fn test_guild_config_default() {
        let config = GuildConfig::default();
        assert_eq!(config.guild_id, 0);
        assert!(config.music_channel_id.is_none());
    }

    #[test]
    fn test_data_debug_impl() {
        let data = Data::new();
        let debug_output = format!("{:?}", data);
        assert!(debug_output.contains("Data"));
        assert!(debug_output.contains("guild_configs"));
        assert!(debug_output.contains("cache"));
    }

    #[test]
    fn test_guild_config_serialization() {
        let config = GuildConfig {
            guild_id: 12345,
            music_channel_id: Some(67890),
        };
        
        // Test serialization
        let serialized = serde_yaml::to_string(&config).expect("Failed to serialize");
        assert!(serialized.contains("guild_id: 12345"));
        assert!(serialized.contains("music_channel_id: 67890"));
        
        // Test deserialization
        let deserialized: GuildConfig = serde_yaml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized.guild_id, 12345);
        assert_eq!(deserialized.music_channel_id, Some(67890));
    }
}

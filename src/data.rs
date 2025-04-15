use poise::serenity_prelude as serenity;
use serde::{Deserialize, Serialize};

/// Guild configuration structure.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GuildConfig {
    // The ID of the guild
    pub guild_id: u64,
    // For example if you're doing a music bot, this could be the ID of the channel
    // where the bot should send music messages.
    pub music_channel_id: Option<u64>,
}

/// Main centrailized data structure for the bot. Should it use the InnerData idiom?
pub struct Data {
    // Map of guild_id -> guild configuration, you'll need one of these for anything more
    // than the most trivial commands.
    pub guild_configs: dashmap::DashMap<serenity::GuildId, GuildConfig>,
    // Cache from the bot's context, you'll probably need this for some commands
    pub cache: serenity::Cache,
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
    pub fn new() -> Self {
        Self {
            guild_configs: dashmap::DashMap::new(),
            cache: serenity::Cache::default(),
        }
    }

    // Load data from YAML file
    pub async fn load() -> Self {
        unimplemented!()
    }

    // Save data to YAML file
    pub async fn save(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!()
    }
}

/// Tests. You should write them.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data() {
        let data = Data::new();
        assert_eq!(data.guild_configs.len(), 0);
    }
}

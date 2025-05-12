use crate::data::Data;
use poise::serenity_prelude::{
    self as serenity, Context, EventHandler, GuildId, Ready, VoiceState,
};
use tracing::{info, warn};

pub struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    /// Called when the bot is ready, but the cache may not be fully populated yet.
    async fn ready(&self, ctx: Context, ready: Ready) {
        let user_name = ready.user.name.clone();
        let shard_id = ctx.shard_id;
        info!("Connected as {user_name}, shard {shard_id}");
    }

    /// Called when the cache is fully populated.
    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        let guild_count_cache = ctx.cache.guild_count();
        let guild_count = guilds.len();
        if guild_count != guild_count_cache {
            warn!(
                "Cache guild count mismatch: {guild_count_cache} (cache) vs {guild_count} (actual)"
            );
        }

        if let Some(data) = {
            let data_read = ctx.data.read().await;
            data_read.get::<Data>().cloned()
        } {
            // Initialize the status tracker with the current data
            info!("Initializing status tracker...");
            data.status.initialize_from_cache(&data);
        } else {
            warn!("Could not get user data from context");
        }

        info!("Cache ready! The bot is in {guild_count} guild(s)");
    }

    /// Called when a user joins, leaves, or moves between voice channels.
    /// We use this to track users in voice channels for status tracking.
    async fn voice_state_update(&self, ctx: Context, old: Option<VoiceState>, new: VoiceState) {
        // Retrieve our data from the context
        let data_lock = {
            let data_read = ctx.data.read().await;
            data_read.get::<Data>().cloned()
        };

        if let (Some(data), Some(guild_id)) = (data_lock, new.guild_id) {
            let user_id = new.user_id;

            // Determine what changed
            match (old.and_then(|vs| vs.channel_id), new.channel_id) {
                // User joined a voice channel
                (None, Some(new_channel)) => {
                    info!("User {user_id} joined voice channel {new_channel} in guild {guild_id}");
                    data.status
                        .user_joined_voice(guild_id, new_channel, user_id, &data);
                }

                // User left a voice channel
                (Some(old_channel), None) => {
                    info!("User {user_id} left voice channel {old_channel} in guild {guild_id}",);
                    data.status.user_left_voice(old_channel, user_id);
                }

                // User moved between voice channels
                (Some(old_channel), Some(new_channel)) if old_channel != new_channel => {
                    info!(
                        "User {user_id} moved from voice channel {old_channel} to {new_channel} in guild {guild_id}",
                    );
                    data.status.user_moved_voice(
                        guild_id,
                        old_channel,
                        new_channel,
                        user_id,
                        &data,
                    );
                }

                // No relevant change or other case
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test the Handler struct can be created
    #[test]
    fn test_handler_creation() {
        let _handler = Handler;
        let _another_handler = Handler;
        assert!(true, "Handler can be created");
    }

    // Since we can't easily mock Context and Ready objects due to their complex structure,
    // we'll test what we can about our handler implementation.
    #[test]
    fn test_handler_implements_event_handler() {
        // This test verifies at compile time that Handler implements EventHandler
        fn assert_impl<T: EventHandler>() {}
        assert_impl::<Handler>();
    }
}

use poise::serenity_prelude::{self as serenity, Context, EventHandler, GuildId, Ready};
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
        info!("Cache ready! The bot is in {guild_count} guild(s)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test the Handler struct can be created
    #[test]
    fn test_handler_creation() {
        let handler = Handler;
        // Just verify we can create the handler
        // Just verify we can create the handler
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

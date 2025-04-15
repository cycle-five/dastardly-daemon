use poise::serenity_prelude::{self as serenity, EventHandler, Context, Ready, GuildId};
use tracing::info;

pub struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Connected as {}, shard {}", ready.user.name, ctx.shard_id);
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        let guild_count_cache = ctx.cache.guild_count();
        let guild_count = guilds.len();
        info!("Cache ready! The bot is in {} guild(s), ready event contains {} guilds.!", guild_count_cache, guild_count);
    }
}
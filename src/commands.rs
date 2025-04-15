use crate::{Data, Error};
use poise::{Context, command};

/// Basic ping command
/// This command is used to check if the bot is responsive.
#[command(prefix_command, slash_command, guild_only)]
pub async fn ping(ctx: Context<'_, Data, Error>) -> Result<(), Error> {
    ctx.say("Pong!").await?;
    Ok(())
}

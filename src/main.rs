mod commands;
mod daemon_response;
mod data;
mod data_ext;
mod enforcement_new;
mod handlers;
mod logging;
mod status;

use crate::data::Data;
use crate::data_ext::DataEnforcementExt;
use std::env;
type Error = Box<dyn std::error::Error + Send + Sync>;

use poise::serenity_prelude::{self as serenity};
use serenity::GatewayIntents;
use tracing::{error, info};

// Customize these constants for your bot
pub const BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";
pub const CONSOLE_TARGET: &str = "dastardly_daemon";

/// Get the Discord bot token from environment variables or a file
///
/// # Returns
/// - A Result containing the token as a String or an Error if it could not be found
///
/// # Errors
/// - Returns an error if neither `DISCORD_TOKEN` nor `DISCORD_TOKEN_FILE`
///   are set in the environment, or if the file cannot be read.
///
fn get_token() -> Result<String, Error> {
    // Try to read the token from environment variables
    env::var("DISCORD_TOKEN")
        .or_else(|_| {
            env::var("DISCORD_TOKEN_FILE").map(|file| {
                std::fs::read_to_string(file)
                    .expect("Failed to read token file")
                    .trim()
                    .to_string()
            })
        })
        .map_err(Into::into)
}

/// Main function to run the bot
async fn async_main() -> Result<(), Error> {
    // Initialize logging
    logging::init(None)?;
    let log_sizes = logging::get_log_sizes("logs")?;
    info!("Log sizes: {log_sizes:?}");

    // Load environment variables
    let token = get_token()?;

    // Load the bot's data from file
    info!("Loading bot data...");
    let mut data = Data::load().await;

    // Initialize the new enforcement system
    info!("Initializing new enforcement system...");
    data.init_enforcement_service();

    // Create enforcement channel and start the task with the new system
    info!("Creating enforcement channel and starting enforcement task...");
    let http: std::sync::Arc<serenity::Http> = serenity::Http::new(&token).into();
    data.import_and_start_enforcement(http.clone(), 60); // Check interval in seconds

    // Keep a clone for the Poise framework below
    let data_cloned = data.clone();

    // Configure the Poise framework
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                // Register commands from commands module
                commands::ping(),
                commands::warn(),
                commands::appease(),
                commands::summon_daemon(),
                commands::daemon_altar(),
                commands::chaos_ritual(),
                commands::judgment_history(),
                commands::daemon_status(),
            ],
            pre_command: |ctx| {
                Box::pin(async move {
                    // Log the start of command execution
                    logging::log_command_start(ctx);
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    // Log the end of command execution
                    logging::log_command_end(ctx);
                })
            },
            on_error: |error| {
                Box::pin(async move {
                    // Log the error using our logging system
                    crate::logging::log_command_error(&error);
                    match error {
                        poise::FrameworkError::Command { error, ctx, ..} => {
                            if let Err(err) = ctx.say(format!("An error occurred: {error}")).await {
                                error!(target: ERROR_TARGET, "Failed to send error message: {err}");
                            }
                        },
                        // TODO: Handle other error types as needed
                        poise::FrameworkError::EventHandler { error, event, .. } => {
                            error!(target: EVENT_TARGET, "Event handler error: {error} in event {event:?}");
                        },
                        _ => {
                            error!(target: ERROR_TARGET, "Error: {error}");
                        }
                    }
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                info!(
                    "Registering commands and return data, this will go away in the next version of poise"
                );
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                // Register the bot's data - clone from the Arc
                Ok(data_cloned.clone())
            })
        })
        .build();

    // Configure the Serenity client
    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_VOICE_STATES;
    let mut client = serenity::ClientBuilder::new(token, intents)
        .event_handler(handlers::Handler)
        .framework(framework)
        .await
        .expect("Failed to create client");

    info!("Starting bot...");

    // Insert bot data into client (for event handlers) - clone from the Arc
    {
        let mut client_data = client.data.write().await;
        client_data.insert::<Data>(data.clone());
    }

    let client_handle = client.start();

    // Wait for Ctrl+C or other termination signal
    tokio::select! {
        result = client_handle => {
            if let Err(err) = result {
                eprintln!("Error running the bot: {err}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
    }

    // Save data before shutting down
    info!("Saving bot data...");
    if let Err(err) = data.save().await {
        eprintln!("Error saving bot data: {err}");
    }

    info!("Bot shutdown complete");
    Ok(())
}

fn main() {
    // Run the async main function
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main());

    // Handle any errors that occurred during execution
    if let Err(err) = result {
        eprintln!("Error: {err}");
    }
}

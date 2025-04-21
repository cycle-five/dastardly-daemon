mod commands;
mod data;
mod handlers;
mod logging;
mod enforcement;

use std::env;

use poise::serenity_prelude::{self as serenity};
use serenity::GatewayIntents;
use tracing::info;

// Customize these constants for your bot
pub const BOT_NAME: &str = "simp_sniper_rs";
pub const COMMAND_TARGET: &str = "simp_sniper_rs::command";
pub const ERROR_TARGET: &str = "simp_sniper_rs::error";
pub const EVENT_TARGET: &str = "simp_sniper_rs::handlers";
pub const CONSOLE_TARGET: &str = "simp_sniper_rs";
pub use data::Data;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Main function to run the bot
async fn async_main() -> Result<(), Error> {
    // Initialize logging
    logging::init()?;

    // Load environment variables
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");

    // Load the bot's data from file
    info!("Loading bot data...");
    let data = Data::load().await;
    let data_clone = data.clone();

    // Configure the Poise framework
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                // Register commands from commands module
                commands::ping(),
                commands::warn(),
                commands::cancelwarning(),
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
                })
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                logging::log_console(
                    "Registering commands and return data, this will go away in the next version of poise"
                );
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                // Register the bot's data
                Ok(data_clone)
            })
        })
        .build();

    // Configure the Serenity client
    let intents = GatewayIntents::non_privileged() | GatewayIntents::GUILD_MODERATION;
    let mut client = serenity::ClientBuilder::new(token, intents)
        .event_handler(handlers::Handler)
        .framework(framework)
        .await
        .expect("Failed to create client");

    info!("Starting bot...");
    
    // Spawn background task to check enforcements
    let data_for_task = data.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60)); // Check every minute
        loop {
            interval.tick().await;
            
            // Check for enforcements that need to be executed
            if let Err(e) = enforcement::check_and_execute_enforcements(&data_for_task).await {
                tracing::error!("Error in enforcement task: {}", e);
            }
        }
    });
    
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

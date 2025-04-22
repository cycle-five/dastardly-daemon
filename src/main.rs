mod commands;
mod data;
mod enforcement;
mod handlers;
mod llm;
mod logging;

use std::env;

use poise::serenity_prelude::{self as serenity};
use serenity::GatewayIntents;
use tracing::{error, info};

// Customize these constants for your bot
pub const BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";
pub const CONSOLE_TARGET: &str = "dastardly_daemon";
pub use data::{Data, DataInner};
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
    let mut data = Data::load().await;

    // Create enforcement channel first
    info!("Creating enforcement channel...");
    let enforcement_tx = enforcement::create_enforcement_channel();

    // Set the enforcement sender in data BEFORE wrapping in Arc
    data.set_enforcement_tx(enforcement_tx);

    // Now wrap the data in Arc for thread-safe sharing
    // let data = Arc::new(data);
    let data_cloned = data.clone();

    // Start the enforcement task with the receiver
    if let Some(rx) = enforcement::take_enforcement_receiver() {
        info!("Starting enforcement task...");
        enforcement::start_task_with_receiver(
            serenity::Http::new(&token).into(),
            data_cloned.clone(),
            rx,
            60, // Check interval in seconds
        );
    } else {
        error!("Failed to get enforcement receiver");
    }

    // Configure the Poise framework
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                // Register commands from commands module
                commands::ping(),
                commands::warn(),
                commands::appease(),      // renamed from cancelwarning
                commands::summon_daemon(), // renamed from vcthumbsdown
                commands::daemon_altar(),  // renamed from setenforcementlog
                commands::chaos_ritual(),  // renamed from setchaos
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
                // Register the bot's data - clone from the Arc
                Ok(data_cloned.clone())
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

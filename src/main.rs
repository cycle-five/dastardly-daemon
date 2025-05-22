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
use tracing::info;

// Customize these constants for your bot
pub const BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";
pub const CONSOLE_TARGET: &str = "dastardly_daemon";

/// Main function to run the bot
async fn async_main() -> Result<(), Error> {
    // Initialize logging
    logging::init(None)?;
    let log_sizes = logging::get_log_sizes("logs")?;
    info!("Log sizes: {log_sizes:?}");

    // Load environment variables
    let token = env::var("DISCORD_TOKEN").unwrap_or_else(|_| {
        env::var("DISCORD_TOKEN_FILE")
            .map(|file| {
                let contents = std::fs::read_to_string(file).expect("Failed to read token file");
                contents.trim().to_string()
            })
            .unwrap_or_else(|_| panic!("DISCORD_TOKEN or DISCORD_TOKEN_FILE not set"))
    });

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
    
    // // For backward compatibility, also initialize the old enforcement system
    // let enforcement_tx = enforcement::create_enforcement_channel();
    // data.set_enforcement_tx(enforcement_tx);
    
    // Keep a clone for the Poise framework below
    let data_cloned = data.clone();
    
    // // For backward compatibility, also start the old enforcement task
    // if let Some(rx) = enforcement::take_enforcement_receiver() {
    //     info!("Starting old enforcement task (for backward compatibility)...");
    //     enforcement::start_task_with_receiver(
    //         http,
    //         data_cloned.clone(),
    //         rx,
    //         60, // Check interval in seconds
    //     );
    // } else {
    //     error!("Failed to get old enforcement receiver");
    // }

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

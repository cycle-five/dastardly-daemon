use crate::Error;
use crate::data::Data;
use poise::{Context, FrameworkError};
use std::path::Path;
use std::time::Instant;
use tracing::{error, info};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

/// Log directory name
pub const DEFAULT_LOG_DIR: &str = "logs";
/// Command log file name
pub const COMMAND_LOG_FILE: &str = "commands";
/// Event log file name
pub const EVENTS_LOG_FILE: &str = "events";
/// You might add other log files here...
pub const _YOUR_OTHER_CONSTS: &str = "ASDF";

// Customize these constants for your bot
pub const _BOT_NAME: &str = "dastardly_daemon";
pub const COMMAND_TARGET: &str = "dastardly_daemon::command";
pub const ERROR_TARGET: &str = "dastardly_daemon::error";
pub const EVENT_TARGET: &str = "dastardly_daemon::handlers";

/// Initialize the logging system with console and file outputs
/// # Errors
/// - Errors if log directory can't be created.
pub fn init(log_dir: Option<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create log directory if it doesn't exist
    let log_dir = log_dir.unwrap_or_else(|| DEFAULT_LOG_DIR.to_string());
    if !Path::new(&log_dir).exists() {
        std::fs::create_dir_all(&log_dir)?;
    }

    // Set up file appenders with daily rotation
    let command_file = RollingFileAppender::new(Rotation::DAILY, &log_dir, COMMAND_LOG_FILE);
    let event_file = RollingFileAppender::new(Rotation::DAILY, &log_dir, EVENTS_LOG_FILE);

    let command_filter = EnvFilter::new(format!("{COMMAND_TARGET}=info"));
    let event_filter = EnvFilter::new(format!("{EVENT_TARGET}=info"));

    // Create a layer for console output (human-readable format)
    let console_layer = fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_ansi(true);

    // Create a layer for command logs (JSON format)
    let command_layer = fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_ansi(false)
        .json()
        .with_writer(command_file)
        .with_filter(command_filter);

    // Create a layer for logs from events
    let event_layer = fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_ansi(false)
        .with_writer(event_file)
        .with_filter(event_filter);

    // Set up the subscriber with all layers
    // Use env filter to allow runtime configuration of log levels
    // Default to INFO level if not specified, but filter out serenity heartbeat logs
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info")
            // Filter out serenity logs
            .add_directive("serenity=error".parse().unwrap_or_default())
    });

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(command_layer)
        .with(event_layer)
        .init();

    info!("Logging system initialized");
    Ok(())
}

// Store command start time in a thread-local variable
thread_local! {
    static COMMAND_START_TIME: std::cell::RefCell<Option<Instant>> = const { std::cell::RefCell::new(None) };
}

/// Log the start of a command execution (pre-command hook)
pub fn log_command_start(ctx: Context<'_, Data, Error>) {
    // Store the start time for later use in post_command
    COMMAND_START_TIME.with(|cell| {
        *cell.borrow_mut() = Some(Instant::now());
    });

    let command_name = ctx.command().qualified_name.clone();
    let guild_id = ctx
        .guild_id()
        .map_or_else(|| "DM".to_string(), |id| id.get().to_string());
    let user_id = ctx.author().id.get().to_string();

    // Attempt to format arguments
    let args = if ctx.command().parameters.is_empty() {
        String::new()
    } else {
        // This is a simplified approach - in a real scenario you'd want to
        // extract the actual arguments more carefully
        format!("{:?}", ctx.invocation_string())
    };

    info!(
        target: COMMAND_TARGET,
        command = %command_name,
        guild_id = %guild_id,
        user_id = %user_id,
        arguments = %args,
        event = "start",
        "Command execution started"
    );
}

/// Log the end of a command execution (post-command hook)
pub fn log_command_end(ctx: Context<'_, Data, Error>) {
    // Calculate execution time
    let duration =
        COMMAND_START_TIME.with(|cell| cell.borrow_mut().take().map(|start| start.elapsed()));

    let command_name = ctx.command().qualified_name.clone();
    let guild_id = ctx
        .guild_id()
        .map_or_else(|| "DM".to_string(), |id| id.get().to_string());
    let user_id = ctx.author().id.get().to_string();

    let duration_ms = u64::try_from(duration.map_or(0, |d| d.as_millis())).unwrap_or_default();
    info!(
        target: COMMAND_TARGET,
        command = %command_name,
        guild_id = %guild_id,
        user_id = %user_id,
        duration_ms = duration_ms,
        event = "end",
        "Command execution completed"
    );
}

/// Log errors that occur during command execution
pub fn log_command_error(error: &FrameworkError<'_, Data, Error>) {
    match error {
        FrameworkError::Command { error, ctx, .. } => {
            let command_name = ctx.command().qualified_name.clone();
            let guild_id = ctx
                .guild_id()
                .as_ref()
                .map_or_else(|| "DM".to_string(), ToString::to_string);
            let user_id = ctx.author().id.get().to_string();

            error!(
                target: ERROR_TARGET,
                command = %command_name,
                guild_id = %guild_id,
                user_id = %user_id,
                error = %error,
                "Command error"
            );
        }
        FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            let command_name = ctx.command().qualified_name.clone();
            let guild_id = ctx
                .guild_id()
                .as_ref()
                .map_or_else(|| "DM".to_string(), ToString::to_string);
            let user_id = ctx.author().id.get().to_string();

            let error_msg = error
                .as_ref()
                .map_or_else(|| "Check failed".to_string(), ToString::to_string);

            error!(
                target: ERROR_TARGET,
                command = %command_name,
                guild_id = %guild_id,
                user_id = %user_id,
                error = %error_msg,
                "Command check failed"
            );
        }
        err => {
            error!(
                target: ERROR_TARGET,
                error_type = %std::any::type_name::<FrameworkError<'_, Data, Error>>(),
                error = ?err,
                "Other framework error"
            );
        }
    }
}

/// Log the size of the command and event log files
/// # Errors
/// - Errors if the log files can't be accessed.
pub fn get_log_sizes(log_dir: String) -> Result<(u64, u64), Error> {
    let command_log_path = format!("{log_dir}/{COMMAND_LOG_FILE}.*");
    let event_log_path = format!("{log_dir}/{EVENTS_LOG_FILE}.*");
    let command_log_paths = glob::glob(&command_log_path)?;
    let event_log_paths = glob::glob(&event_log_path)?;

    let command_logs_size = command_log_paths
        .filter_map(|entry| entry.ok())
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum::<u64>();
    let event_logs_size = event_log_paths
        .filter_map(|entry| entry.ok())
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|meta| meta.len())
        .sum::<u64>();

    Ok((command_logs_size, event_logs_size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Once;

    // Ensure init() is only called once in tests
    static INIT: Once = Once::new();

    fn setup() {
        INIT.call_once(|| {
            // Use a test-specific log directory to avoid conflicts
            const TEST_LOG_DIR: &str = "test_logs";

            // Clean up any existing test logs
            if Path::new(TEST_LOG_DIR).exists() {
                let _ = std::fs::remove_dir_all(TEST_LOG_DIR);
            }

            // Initialize logging with test configuration
            let _ = init(Some(TEST_LOG_DIR.to_string()));
        });
    }

    #[test]
    fn test_get_log_sizes() {
        setup();

        info!("Testing log sizes...");

        // Test that log_console doesn't panic
        let (command_log_size, event_log_size) = get_log_sizes("test_logs".to_string()).unwrap();
        assert_eq!(command_log_size, 0);
        assert_eq!(event_log_size, 0);
    }

    #[test]
    fn test_thread_local_command_start_time() {
        // Test that the thread local variable can be accessed
        COMMAND_START_TIME.with(|cell| {
            assert!(cell.borrow().is_none());
            *cell.borrow_mut() = Some(Instant::now());
            assert!(cell.borrow().is_some());
        });
    }
}

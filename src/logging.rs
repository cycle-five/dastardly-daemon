use crate::{COMMAND_TARGET, CONSOLE_TARGET, ERROR_TARGET, EVENT_TARGET};
use crate::{Data, Error};
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
pub const LOG_DIR: &str = "logs";
/// Command log file name
pub const COMMAND_LOG_FILE: &str = "commands";
/// Event log file name
pub const EVENTS_LOG_FILE: &str = "events";
/// You might add other log files here...
/// Initialize the logging system with console and file outputs
pub fn init() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create log directory if it doesn't exist
    if !Path::new(LOG_DIR).exists() {
        std::fs::create_dir_all(LOG_DIR)?;
    }

    // Set up file appenders with daily rotation
    let command_file = RollingFileAppender::new(Rotation::DAILY, LOG_DIR, COMMAND_LOG_FILE);
    let event_file = RollingFileAppender::new(Rotation::DAILY, LOG_DIR, EVENTS_LOG_FILE);

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
            .add_directive("serenity=error".parse().unwrap())
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

/// Log messages to the console
pub fn log_console(message: &str) {
    info!(
        target: CONSOLE_TARGET,
        message = %message,
        event = "console",
    );
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
            let _ = init();
        });
    }

    #[test]
    fn test_log_console() {
        setup();

        // Test that log_console doesn't panic
        log_console("Test message");
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

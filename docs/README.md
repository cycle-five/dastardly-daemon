# Discord Bot Template in Rust

A robust, modular Discord bot template built with Rust, using Serenity and Poise frameworks.

## Features

- **Modular Architecture**: Clean separation of concerns with dedicated modules for commands, event handlers, data management, and logging
- **Comprehensive Logging**: Structured logging with file and console outputs
- **Command Framework**: Built on Poise for easy command creation with slash command support
- **Persistent Data**: Framework for guild-specific configuration storage
- **Error Handling**: Robust error handling throughout the application
- **Unit Tests**: Comprehensive test coverage for all modules

## Project Structure

```
bot-template-rs/
├── src/
│   ├── commands.rs    # Bot commands implementation
│   ├── data.rs        # Data structures and persistence
│   ├── handlers.rs    # Event handlers for Discord events
│   ├── logging.rs     # Logging configuration and utilities
│   └── main.rs        # Application entry point
├── Cargo.toml         # Project dependencies
└── README.md          # Project documentation
```

## Getting Started

### Prerequisites

- Rust (latest stable version)
- A Discord bot token

### Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/bot-template-rs.git
   cd bot-template-rs
   ```

2. Set up your Discord bot token:
   ```bash
   export DISCORD_TOKEN=your_token_here
   ```

3. Build and run the bot:
   ```bash
   cargo run
   ```

## Usage

### Adding Commands

Add new commands in `commands.rs`:

```rust
/// Example command that returns the current time
#[command(prefix_command, slash_command)]
pub async fn time(ctx: Context<'_, Data, Error>) -> Result<(), Error> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    ctx.say(format!("The current time is: {}", now)).await?;
    Ok(())
}
```

Then register the command in `main.rs`:

```rust
commands: vec![
    commands::ping(),
    commands::time(),  // Add your new command here
],
```

### Event Handling

The bot includes handlers for common events:

- `ready`: Called when the bot connects to Discord
- `cache_ready`: Called when the cache is fully populated with guild information

Add custom event handlers in `handlers.rs` by extending the `EventHandler` implementation.

### Data Management

The `Data` struct in `data.rs` provides a centralized data store for your bot. It includes:

- Guild-specific configurations
- Cache access

Extend the `GuildConfig` struct to store additional guild-specific settings.

## Logging

The bot uses the `tracing` crate for structured logging:

- Console output for development
- JSON file output for production
- Separate log files for commands and events
- Daily log rotation

Logs are stored in the `logs/` directory.

## Testing

Run the test suite with:

```bash
cargo test
```

The project includes unit tests for all modules:

- `handlers.rs`: Tests for event handlers
- `commands.rs`: Tests for command definitions
- `data.rs`: Tests for data structures and serialization
- `logging.rs`: Tests for logging functionality

## Extending the Bot

### Adding New Features

1. **New Command Category**: Create a new module in `src/` and register it in `main.rs`
2. **Custom Events**: Add new event handlers in `handlers.rs`
3. **Additional Data**: Extend the `Data` and `GuildConfig` structs in `data.rs`

### Deployment

For production deployment:

1. Build in release mode:
   ```bash
   cargo build --release
   ```

2. Set up environment variables or a `.env` file for configuration
3. Use a process manager like systemd or Docker for running the bot

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
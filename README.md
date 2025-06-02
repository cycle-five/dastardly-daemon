# Dastardly Daemon

[![codecov](https://codecov.io/gh/cycle-five/dastardly-daemon/branch/master/graph/badge.svg)](https://codecov.io/gh/cycle-fove/dastardly-daemon)

A Discord moderation bot with a twist - an unpredictable daemon that moderates voice channels according to its own chaotic whims.

## Overview

Dastardly Daemon is a Discord bot designed to help moderate voice channels with an erratic and unpredictable personality. The daemon can be summoned by moderators to warn users about inappropriate behavior, but the way it dishes out punishments is inconsistent and sometimes random.

## Features

- **🎭 Demonic Personality**: The daemon decides when and how to dish out punishments, sometimes being lenient and other times harsh
- **🎲 Chaos Factor**: Configure how unpredictable the daemon's behavior will be 
- **👻 Voice Channel Haunting**: The daemon can teleport users between voice channels randomly
- **🔮 Configurable Responses**: Custom daemon-themed messages (with optional LLM integration)
- **⚡ Automatic Enforcement**: After a certain number of warnings, the daemon will take action

## Commands

| Command | Description |
|---------|-------------|
| `/summon_daemon` | Call the daemon to judge a user's voice behavior |
| `/warn` | Issue a standard warning to a user |
| `/appease` | Try to convince the daemon to cancel a punishment |
| `/daemon_altar` | Set the channel where the daemon will send its messages |
| `/chaos_ritual` | Adjust the daemon's chaos factor (randomness) |
| `/ping` | Check if the daemon is responsive |

## Enforcement Actions

The daemon has several ways to torment misbehaving users:

- **Voice Mute**: Prevent a user from speaking in voice channels
- **Voice Deafen**: Prevent a user from hearing others in voice channels
- **Voice Disconnect**: Forcibly disconnect a user from voice
- **Voice Channel Haunting**: Teleport a user between random voice channels
- **Server Mute**: Prevent a user from sending messages in text channels
- ~~**Ban**: Temporarily ban a user from the server~~
- ~~**Kick**: Remove a user from the server~~
- **Ban / Kick** have been determined to be out of scope for a VC moderation daemon.

## Getting Started

1. Invite the bot to your server
2. Use `/daemon_altar` to set up a log channel
3. Use `/chaos_ritual` to set the daemon's chaos level (0.0-1.0)
4. Start moderating with `/summon_daemon` and `/warn`

## Daemon Personality

The daemon is:
- **Unpredictable**: Sometimes harsh, sometimes lenient
- **Easily Bored**: May change punishments midway through
- **Playful Tormentor**: Enjoys teleporting users between channels
- **Easily Distracted**: Sometimes forgets what it was doing
- **Grudge Holder**: Remembers repeat offenders

## LLM Integration

Optionally, the daemon can be connected to an LLM to generate more creative and dynamic responses to situations. This is controlled by the `llm` feature flag.

## Development

### Prerequisites

- Rust (latest stable)
- cargo-llvm-cov for code coverage: `cargo install cargo-llvm-cov`

### Building and Testing

```bash
# Build the project
make build

# Run tests
make test

# Run linter
make lint

# Format code
make format
```

### Code Coverage

The project uses `cargo-llvm-cov` for code coverage analysis. Coverage reports are automatically generated in CI and uploaded to Codecov.

To run coverage locally:

```bash
# Generate coverage report in terminal
make coverage

# Generate HTML coverage report
make coverage-html

# Generate and open HTML coverage report in browser
make coverage-open

# Generate LCOV format report (for CI/external tools)
make coverage-lcov
```

The HTML coverage report will be generated in `target/llvm-cov/html/index.html`.

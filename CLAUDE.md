# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands
- Build: `cargo build` or `make build`
- Run: `cargo run`
- Test all: `cargo test` or `make test`
- Test specific: `cargo test <test_name>`
- Example: `cargo test test_data_new`
- Lint: `cargo clippy -- -D clippy::all -D warnings -W clippy::pedantic` or `make lint`
- Format: `cargo fmt` or `make format`

## Code Coverage Commands
- Coverage report (terminal): `cargo llvm-cov --all-features --workspace` or `make coverage`
- Coverage report (HTML): `cargo llvm-cov --all-features --workspace --html` or `make coverage-html`
- Coverage report (HTML + open): `make coverage-open`
- Coverage report (LCOV format): `cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info` or `make coverage-lcov`
- Clean coverage data: `make clean`

## Code Style Guidelines
- Rust Edition: 2024
- Use explicit types for function arguments and returns
- Document public APIs with `///` doc comments and `#[must_use]` where appropriate
- Error handling: Return `Result<T, E>` and propagate errors with `?`
- Struct fields: use public fields (`pub`) for simple data structs
- DRY principle: Implement `Default` trait and use `..Default::default()` for partial construction
- Testing: Write unit tests in a `mod tests` block marked with `#[cfg(test)]`
- Imports: Group by standard lib, external crates, then internal modules
- Formatting: Keep line length under 100 characters
- Use `dashmap` for thread-safe concurrent access
- Async/await for non-blocking operations with tokio
- Use Arc<T> for shared ownership rather than Rc<T>
- String appending: Prefer `write!()` macro over `push_str()` when appending string literals to avoid unnecessary allocations
- Prefer `map_or_else` over `map(...).unwrap_or_else(...)`
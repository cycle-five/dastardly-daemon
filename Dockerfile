FROM rust:1.86-slim AS builder

# Install dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# 
# Make working directory and create non-root user
# 
RUN useradd -ms /bin/sh c5run && \
    mkdir -p /app/logs /app/data && \
    chown -R c5run:c5run /app
# Switch to non-root user
USER c5run

WORKDIR /app
# Copy manifests
COPY entrypoint.sh Cargo.toml Cargo.lock ./

# Cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src/ src/

# Build the application
RUN cargo build --release

# Runtime image
FROM debian:bookworm-20250428-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user and directories with proper permissions
RUN useradd -ms /bin/sh c5run && \
    mkdir -p /app/logs /app/data && \
    chown -R c5run:c5run /app

# Switch to non-root user
USER c5run
WORKDIR /app

# Copy the binary from the builder stage
COPY --from=builder /app/target/release/dastardly-daemon /app/dastardly-daemon
COPY --from=builder /app/entrypoint.sh /app/entrypoint.sh

# Set environment variables
ENV RUST_LOG=info

# Entry point to set up the environment
ENTRYPOINT [ "/app/entrypoint.sh" ]
# Run the binary
CMD ["/app/dastardly-daemon"]
# Multi-stage build for benchScale
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    pkgconfig

WORKDIR /build

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release --bin benchscale

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    docker-cli \
    openssh-client

# Create benchscale user
RUN addgroup -g 1000 benchscale && \
    adduser -D -u 1000 -G benchscale benchscale

# Copy binary from builder
COPY --from=builder /build/target/release/benchscale /usr/local/bin/benchscale

# Create directories
RUN mkdir -p /var/lib/benchscale /etc/benchscale && \
    chown -R benchscale:benchscale /var/lib/benchscale /etc/benchscale

# Switch to benchscale user
USER benchscale

# Set environment variables
ENV BENCHSCALE_STATE_DIR=/var/lib/benchscale
ENV RUST_LOG=info

# Expose documentation port (if needed)
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD benchscale version || exit 1

# Default command
ENTRYPOINT ["benchscale"]
CMD ["--help"]


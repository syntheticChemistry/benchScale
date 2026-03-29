# SPDX-License-Identifier: AGPL-3.0-only
# Multi-stage build for benchScale
FROM rust:1.85-alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    pkgconfig

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src

RUN cargo build --release --bin benchscale

FROM alpine:3.21

RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    docker-cli \
    openssh-client \
    iproute2

RUN addgroup -g 1000 benchscale && \
    adduser -D -u 1000 -G benchscale benchscale

COPY --from=builder /build/target/release/benchscale /usr/local/bin/benchscale

RUN mkdir -p /var/lib/benchscale /etc/benchscale && \
    chown -R benchscale:benchscale /var/lib/benchscale /etc/benchscale

USER benchscale

ENV BENCHSCALE_STATE_DIR=/var/lib/benchscale
ENV RUST_LOG=info

EXPOSE 9200

ENTRYPOINT ["benchscale"]
CMD ["server", "--port", "9200"]

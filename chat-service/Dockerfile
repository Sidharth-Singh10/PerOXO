# syntax=docker/dockerfile:1

FROM rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY src ./src

COPY proto ./proto

COPY build.rs ./

RUN cargo build --release --locked

# Download the health probe once, verifying it against a pinned digest. The
# release checksums file names the binary without a path, so checking it
# requires naming the downloaded file explicitly.
FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3 AS probe

ARG PROBE_VERSION=v0.4.24
ARG PROBE_SHA256=7e564681110ee4563637457b91e42f62f96b79618a835bb05ae2305acdcc3db0

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/* \
    && wget -qO /tmp/grpc_health_probe "https://github.com/grpc-ecosystem/grpc-health-probe/releases/download/${PROBE_VERSION}/grpc_health_probe-linux-amd64" \
    && echo "${PROBE_SHA256}  /tmp/grpc_health_probe" | sha256sum -c - \
    && chmod +x /tmp/grpc_health_probe

FROM debian:bookworm-slim@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 app \
    && useradd --system --uid 10001 --gid app --no-create-home app

WORKDIR /app

COPY --from=builder /app/target/release/chat-service /app/chat-service
COPY --from=probe /tmp/grpc_health_probe /usr/local/bin/grpc_health_probe

USER 10001:10001

EXPOSE 50052

CMD ["/app/chat-service"]

FROM rust:latest as builder

RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

COPY src ./src

COPY proto ./proto

COPY build.rs ./

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/per_oxo /app/per_oxo

EXPOSE 3000

CMD ["./per_oxo"]
FROM rust:1.77-alpine AS builder

RUN apk add --no-cache musl-dev gcc lz4-dev openssl-dev zstd-dev pkgconfig

WORKDIR /app

COPY Cargo.toml ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/ship_simulation_backend*

COPY src ./src
COPY config ./config

RUN cargo build --release

FROM alpine:3.19

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app
COPY --from=builder /app/target/release/ship_simulation_backend /app/ship_simulation_backend
COPY --from=builder /app/config /app/config

ENV SHIP_CONFIG_PATH=/app/config/ship_config.json
ENV DAMAGE_PARAMS_PATH=/app/config/damage_params.json
ENV RUST_LOG=info
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

EXPOSE 8080

CMD ["/app/ship_simulation_backend"]

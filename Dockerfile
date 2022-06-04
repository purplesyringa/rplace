FROM rust:bullseye as chef
RUN apt-get update && apt-get install ca-certificates && rm -rf /var/lib/apt/lists/*
RUN rustup install nightly && rustup default nightly
RUN cargo install cargo-chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder 
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo install --path . --root /output

FROM debian:bullseye-slim
WORKDIR /app

COPY --from=builder /output/bin/rplace /app/rplace
COPY docker/Rocket.toml /app/Rocket.toml
COPY static /app/static

ENTRYPOINT ["/app/rplace"]


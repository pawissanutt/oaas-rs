FROM lukemathwalker/cargo-chef:0.1.71-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG COOK_OPTION="--release"
RUN apt update && apt install -y protobuf-compiler
COPY --from=planner /app/recipe.json .
RUN cargo chef cook ${COOK_OPTION} --recipe-path recipe.json
COPY . .
ARG BUILD_OPTION="--release"
RUN cargo build ${BUILD_OPTION}

FROM debian:stable-slim AS runtime
ARG APP_NAME="oprc-gateway"
WORKDIR /app
COPY --from=builder /app/target/release/${APP_NAME} /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
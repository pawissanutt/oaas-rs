FROM lukemathwalker/cargo-chef:latest AS chef
WORKDIR /app

FROM chef AS planner
# COPY ./Cargo.toml ./Cargo.lock ./
COPY . .
RUN cargo chef prepare

FROM chef AS builder
ARG COOK_OPTION="--release"
ARG BUILD_OPTION="--release"
RUN apt update && apt install -y protobuf-compiler
COPY --from=planner /app/recipe.json .
RUN cargo chef cook ${COOK_OPTION}
COPY . .
RUN cargo build ${BUILD_OPTION}

FROM debian:stable-slim AS runtime
ARG APP_NAME="oprc-gateway"
WORKDIR /app
COPY --from=builder /app/target/release/${APP_NAME} /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
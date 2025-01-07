FROM lukemathwalker/cargo-chef:latest AS chef
WORKDIR /app

FROM chef AS planner
# COPY ./Cargo.toml ./Cargo.lock ./
COPY . .
RUN cargo chef prepare

FROM chef AS builder
ARG COOK_OPTION=""
ARG BUILD_OPTION=""
RUN apt update && apt install -y protobuf-compiler
COPY --from=planner /app/recipe.json .
RUN cargo chef cook ${COOK_OPTION}
COPY . .
RUN cargo build ${BUILD_OPTION}

FROM debian:stable-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/debug/oprc-* /usr/local/bin/
COPY --from=builder /app/target/debug/dev-* /usr/local/bin/
ENTRYPOINT ["/bin/bash"]
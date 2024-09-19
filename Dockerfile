FROM lukemathwalker/cargo-chef:latest as chef
WORKDIR /app

FROM chef AS planner
# COPY ./Cargo.toml ./Cargo.lock ./
COPY . .
RUN cargo chef prepare

FROM chef AS builder
RUN apt update && apt install -y protobuf-compiler
COPY --from=planner /app/recipe.json .
RUN cargo chef cook --release
COPY . .
RUN cargo build --release

FROM debian:stable-slim AS runtime
ARG APP_NAME="oprc-gateway"
WORKDIR /app
COPY --from=builder /app/target/release/${APP_NAME} /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
CMD [ "server", "-p", "80"]v
ARG BUILD_PROFILE=release
FROM lukemathwalker/cargo-chef:0.1.72-rust-1.89.0-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BUILD_PROFILE="release"
RUN apt update && apt install -y protobuf-compiler
COPY --from=planner /app/recipe.json .
RUN cargo chef cook $([ "$BUILD_PROFILE" = "release" ] && echo --release) --recipe-path recipe.json
COPY . .
RUN cargo build $([ "$BUILD_PROFILE" = "release" ] && echo --release)

# Runtime base images: distroless for release, debian for debug
FROM gcr.io/distroless/cc-debian12 AS runtime-release

FROM debian:bookworm-slim AS runtime-debug
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

FROM runtime-${BUILD_PROFILE} AS runtime
ARG BUILD_PROFILE="release"
ARG APP_NAME="oprc-pm"
WORKDIR /app

COPY --from=builder /app/target/${BUILD_PROFILE}/${APP_NAME} /usr/local/bin/app

ENTRYPOINT ["/usr/local/bin/app"]
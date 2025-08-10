FROM rust:1.89-slim-bookworm

RUN apt update && apt install -y protobuf-compiler
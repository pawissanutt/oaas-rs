FROM rust:1.93-slim-bookworm

RUN apt update && apt install -y protobuf-compiler
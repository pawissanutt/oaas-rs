FROM rust:1

RUN apt update && apt install -y protobuf-compiler
#!/usr/bin/env bash

cargo run --bin bench-kv-set --features loadtest -r -- example 12 -s 3 -c 1000 -d 10s
cargo run --bin bench-kv-get --features loadtest -r -- example 12 -s 3 -c 1000 -d 10s
#!/usr/bin/env bash

ODGM_BIN="${ODGM_BIN:-oprc-odgm}"
ROUTER_BIN="${ROUTER_BIN:-oprc-router}"

pkill -f "$ODGM_BIN"
pkill -f "$ROUTER_BIN"

#!/usr/bin/env bash

set -e

echo "*** Initializing WASM build environment"

rustup target add wasm32-unknown-unknown --toolchain nightly

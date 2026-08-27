#!/bin/sh
set -eu

cd "$(dirname "$0")"

cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target x86_64-pc-windows-gnu

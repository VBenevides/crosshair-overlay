#!/bin/sh
set -u

cd "$(dirname "$0")"

failures=

try_build() {
    name=$1
    target=$2
    if ! cargo build --release --target "$target"; then
        failures="$failures $name"
    fi
}

try_build linux x86_64-unknown-linux-gnu
try_build windows x86_64-pc-windows-gnu

if [ -n "$failures" ]; then
    printf 'Failed builds:%s\n' "$failures"
    exit 1
fi

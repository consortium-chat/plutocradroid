#!/bin/bash
cd "$(dirname "$0")"
cargo build --features debug && scp target/debug/plutocradroid shelvacu.com:pluto2 #&& ssh shelvacu.com 'cd pluto2 && RUST_LOG=info ./plutocradroid web'

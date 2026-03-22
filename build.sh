#!/bin/bash
# ratatui 0.28 depends on instability/darling which declare MSRV 1.88, but the
# code compiles fine on 1.87. Use --ignore-rust-version to bypass the check.
#cargo build --release --ignore-rust-version "$@"
cargo build --release  --target x86_64-unknown-linux-musl
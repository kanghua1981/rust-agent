#!/bin/sh
stop_agent.sh
cp target/x86_64-unknown-linux-musl/release/agent ~/bin/
start_agent.sh
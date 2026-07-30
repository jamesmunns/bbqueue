#!/bin/bash

# We disable isolation because we use tokio sleep
MIRIFLAGS="-Zmiri-disable-isolation" \
    cargo +nightly miri test \
    --target x86_64-unknown-linux-gnu \
    --features=std

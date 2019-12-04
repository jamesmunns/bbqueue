set -euxo pipefail

# Install embedded target for no_std test
rustup target add thumbv7em-none-eabihf

# Check with no_std target to make sure it really works for embedded/no_std
cargo build --target thumbv7em-none-eabihf --manifest-path core/Cargo.toml

# Check doctests
cargo test --manifest-path core/Cargo.toml --no-default-features --features atomic
cargo test --manifest-path core/Cargo.toml --no-default-features --features thumbv6

export RUSTFLAGS="-Z sanitizer=thread"
export RUST_TEST_THREADS=1
export TSAN_OPTIONS="suppressions=$(pwd)/tsan-blacklist.txt"

# Test using a full std crate, short test with multiple threads (it's slow)
cargo test --features="travisci" --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture
cargo test --features="travisci" --release --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture

# Test using a full std crate, long test with a single thread (it's faster)
cargo test --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture --test-threads=1
cargo test --release --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture --test-threads=1


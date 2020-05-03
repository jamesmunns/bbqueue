set -euxo pipefail

# Install embedded target for no_std test
rustup target add thumbv7em-none-eabihf

# Try to ensure source code formatting. If rustfmt isn't
# available, oh well.
if rustup component add rustfmt ; then
    cargo fmt -- --check
else
    echo "No cargo fmt for us today :("
fi

# Check with no_std target to make sure it really works for embedded/no_std
cargo build --target thumbv7em-none-eabihf --manifest-path core/Cargo.toml

# Check doctests
cargo test --manifest-path core/Cargo.toml --no-default-features --features atomic
cargo test --manifest-path core/Cargo.toml --no-default-features --features thumbv6

if [ $TRAVIS_RUST_VERSION = nightly ]; then
    # Run ONLY the short test, also with a sanitizer

    export RUSTFLAGS="-Z sanitizer=thread"
    export RUST_TEST_THREADS=1
    export TSAN_OPTIONS="suppressions=$(pwd)/tsan-blacklist.txt"

    cargo test --features="travisci,nightly" --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture
    cargo test --features="travisci,nightly" --release --manifest-path bbqtest/Cargo.toml --target x86_64-unknown-linux-gnu -- --nocapture
else
    # Test using a full std crate, short test with multiple threads (it's slow)
    cargo test --features="travisci" --manifest-path bbqtest/Cargo.toml -- --nocapture
    cargo test --features="travisci" --release --manifest-path bbqtest/Cargo.toml -- --nocapture

    # Test using a full std crate, long test with a single thread (it's faster)
    cargo test --features="verbose" --manifest-path bbqtest/Cargo.toml -- --nocapture --test-threads=1
    cargo test --features="verbose" --release --manifest-path bbqtest/Cargo.toml -- --nocapture --test-threads=1
fi


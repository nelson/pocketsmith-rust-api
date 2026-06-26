# justfile — common dev/release tasks for the `pocketsmith` binary.
# Run `just` to list recipes. Requires https://github.com/casey/just

# Build the optimized binary with the web UI compiled in.
build:
    cargo build --release --features web

# Install the `pocketsmith` binary (with the web UI) into ~/.cargo/bin.
install: build
    cargo install --path . --features web --force

# Cut a release: push the current branch so release-please opens/updates the
# release PR; merging that PR tags the version and CI builds the musl tarball.
release:
    git push origin HEAD

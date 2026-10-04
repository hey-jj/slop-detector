#!/usr/bin/env bash
# MSRV gate for the declared rust-version (1.85).
#
# The shipped Cargo.lock is inert for dependents. Their cargo resolves
# the graph fresh from the index. This gate builds a real external consumer
# with a path dependency on slop-detector and no carried lockfile on Rust
# 1.85. This matches a dependent at the MSRV floor. It then builds the crate
# itself at 1.85 to prove its code compiles at the declared floor.
#
# Run it with a 1.85.x toolchain active (e.g. `rustup run 1.85.0 ci/msrv-consumer.sh`).
set -euo pipefail

CRATE_DIR="$(cd "$(dirname "$0")/.." && pwd)"

RUSTC_VERSION="$(rustc --version)"
echo "active toolchain: $RUSTC_VERSION"
case "$RUSTC_VERSION" in
*" 1.85."*) ;;
*)
    echo "error: this gate must run on Rust 1.85.x (the declared MSRV); got: $RUSTC_VERSION" >&2
    exit 1
    ;;
esac

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT
CONSUMER="$WORKDIR/msrv-consumer"
mkdir -p "$CONSUMER/src"

cat >"$CONSUMER/Cargo.toml" <<TOML
[package]
name = "msrv-consumer"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
slop-detector = { path = "$CRATE_DIR" }
TOML

cat >"$CONSUMER/src/main.rs" <<'RS'
fn main() {
    let report = slop_detector::analyze("MSRV consumer smoke input.\n");
    println!(
        "slop-detector MSRV consumer ok: {} residue, {} quality, {} injection, {} words",
        report.paste_residue.len(),
        report.quality_patterns.len(),
        report.injection_patterns.len(),
        report.stats.word_count
    );
}
RS

# $CONSUMER starts without a lockfile, so cargo resolves from the index,
# as any dependent would. A transitive dep whose current version needs >1.85
# fails this build even though slop-detector's own pinned lockfile builds fine.
echo "== external consumer: fresh-resolution build at 1.85 =="
cargo build --manifest-path "$CONSUMER/Cargo.toml"

echo "== external consumer: run =="
cargo run --quiet --manifest-path "$CONSUMER/Cargo.toml"

echo "== slop-detector itself: build at 1.85 (own lockfile) =="
cargo build --manifest-path "$CRATE_DIR/Cargo.toml"

echo "MSRV gate passed at: $RUSTC_VERSION"

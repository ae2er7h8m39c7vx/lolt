#!/usr/bin/env bash
#
# Builds liblolt_core.a for the architectures Xcode asked for and lipos them
# into target/universal-apple-darwin/release/, where both Xcode targets look.
#
# Runs as a pre-build phase, and works standalone:
#     ./scripts/build-rust.sh              # universal (arm64 + x86_64)
#     ARCHS=arm64 ./scripts/build-rust.sh  # just this Mac
#
set -euo pipefail

# Xcode's PATH is minimal and won't include rustup's shims.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR="$CRATE_ROOT/target/universal-apple-darwin/release"
LIB="liblolt_core.a"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found. Install Rust from https://rustup.rs, then rebuild." >&2
    exit 1
fi

# Xcode sets ARCHS to what it's building; fall back to a universal library.
targets=()
for arch in ${ARCHS:-arm64 x86_64}; do
    case "$arch" in
        arm64)  targets+=("aarch64-apple-darwin") ;;
        x86_64) targets+=("x86_64-apple-darwin") ;;
        *)      echo "warning: skipping unknown architecture '$arch'" >&2 ;;
    esac
done

if [ ${#targets[@]} -eq 0 ]; then
    echo "error: no supported architectures in ARCHS='${ARCHS:-}'" >&2
    exit 1
fi

installed="$(rustup target list --installed 2>/dev/null || true)"
slices=()
for target in "${targets[@]}"; do
    if ! grep -qx "$target" <<<"$installed"; then
        echo "note: installing Rust target $target"
        rustup target add "$target"
    fi
    # --lib skips the CLI binary; the widget only needs the static library.
    (cd "$CRATE_ROOT" && cargo build --release --lib --target "$target")
    slices+=("$CRATE_ROOT/target/$target/release/$LIB")
done

mkdir -p "$OUT_DIR"
lipo -create "${slices[@]}" -output "$OUT_DIR/$LIB"
echo "built $OUT_DIR/$LIB ($(lipo -archs "$OUT_DIR/$LIB"))"

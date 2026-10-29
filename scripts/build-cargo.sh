#!/bin/bash
# Build cargo (rust/src/tools/cargo) for x86_64-unknown-plan9 with the stage-1 rustc, the
# crates of crates/ (cargo-plan9.toml) and 9front-gcc's cross gcc for the C in it (APE).
# Result: tests/out/cargo-port/x86_64-unknown-plan9/release/cargo; log: logs/cargo-port.log.
TOP=$(cd "$(dirname "$0")/.." && pwd)
GCC9=${GCC9:-$TOP/../9front-gcc}
export PATH=$GCC9/build/amd64/cross/bin:$PATH
export RUSTC=$TOP/rust/build/host/stage1/bin/rustc
export CARGO_TARGET_DIR=$TOP/tests/out/cargo-port
export CC_x86_64_unknown_plan9=x86_64-pc-plan9-gcc CXX_x86_64_unknown_plan9=x86_64-pc-plan9-g++
# C against APE, renamed to build/ape9's names when archived; linked with build/ape9
[ -f "$TOP/build/ape9/cflags" ] || "$TOP/scripts/ape9.sh"
export AR_x86_64_unknown_plan9=$TOP/scripts/ape9-ar
# libz-sys: its own zlib (APE's is old)
export LIBZ_SYS_STATIC=1
export CFLAGS_x86_64_unknown_plan9="$(cat "$TOP/build/ape9/cflags")"
export CARGO_TARGET_X86_64_UNKNOWN_PLAN9_LINKER=x86_64-pc-plan9-gcc
export CARGO_TARGET_X86_64_UNKNOWN_PLAN9_RUSTFLAGS=$(for a in $(cat "$TOP/build/ape9/link.args"); do printf -- '-Clink-arg=%s ' "$a"; done)
cargo build --keep-going --release --target x86_64-unknown-plan9 \
	--manifest-path "$TOP/rust/src/tools/cargo/Cargo.toml" \
	--config "$TOP/cargo-plan9.toml" "$@" > "$TOP/logs/cargo-port.log" 2>&1
r=$?
grep -E '^error' "$TOP/logs/cargo-port.log" | sort | uniq -c | head -40
exit $r

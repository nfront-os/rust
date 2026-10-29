#!/bin/bash
# rustc on 9front: the stage-2 rustc for x86_64-unknown-plan9 (static, its LLVM inside) as
# /amd64/rust/bin/rustc with std in /amd64/rust/lib/rustlib, and 9front-gcc's native gcc
# (/amd64/gnu, the linker) in the guest; compiles and runs tests/std1.rs there.
# Prints "name ok" or "name FAIL ...". Log: logs/rustc-guest.log.
# env: GCC9OUT (the native toolchain, default ../9front-gcc/out-gcc), MEM (guest MB, 3072),
# PLAN9 (the system booted: its APE and libp9.a link the program; default ../9front-gccwt)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
export PLAN9=${PLAN9:-$TOP/../9front-gccwt}
GCC9=${GCC9:-$TOP/../9front-gcc}
GCC9OUT=${GCC9OUT:-$GCC9/out-gcc}
GCC9BUILD=${GCC9BUILD:-$GCC9/build-gcc}
X=$GCC9BUILD/amd64/cross/bin/x86_64-pc-plan9
R=$TOP/rust/build
I=$TOP/build/rustc-plan9
rm -rf "$I"
mkdir -p "$I/bin" "$I/lib/rustlib/x86_64-unknown-plan9/lib"
"$X-strip" -o "$I/bin/rustc" "$R/x86_64-unknown-plan9/stage2/bin/rustc"
cp "$R"/x86_64-unknown-linux-gnu/stage2/lib/rustlib/x86_64-unknown-plan9/lib/*.rlib \
	"$I/lib/rustlib/x86_64-unknown-plan9/lib/"
args=()
for f in $(cd "$I" && find . -type f); do args+=(-x "/amd64/rust/${f#./}=$I/$f"); done
for f in $(cd "$GCC9OUT/amd64/amd64/gnu" && find . -type f); do
	args+=(-x "/amd64/gnu/${f#./}=$GCC9OUT/amd64/amd64/gnu/$f")
done
cmds=(
	"bind -b /amd64/gnu/bin /bin"
	"/amd64/rust/bin/rustc --version; if(~ \$status '') echo version ok; if not echo version FAIL \$status"
	"cd /tmp; /amd64/rust/bin/rustc --edition 2024 -O -C linker=gcc /tests/std1.rs -o std1 >[2=1] | sed 20q; if(test -x std1) echo compile ok; if not echo compile FAIL"
	"/tmp/std1 a b; if(~ \$status '') echo run ok; if not echo run FAIL \$status"
)
MEM=${MEM:-3072} GUEST_TIMEOUT=${GUEST_TIMEOUT:-3000} "$TOP/tests/guest.sh" "${args[@]}" -f "$TOP/tests/std1.rs" -- "${cmds[@]}" \
	> "$TOP/logs/rustc-guest.log" 2>&1
grep -E '^(version|compile|run) (ok|FAIL)' "$TOP/logs/rustc-guest.log" || { tail -30 "$TOP/logs/rustc-guest.log"; exit 1; }

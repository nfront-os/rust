#!/bin/bash
# rustc on 9front: the stage-2 rustc for x86_64-unknown-plan9 (static, its LLVM inside) as
# /amd64/rust/bin/rustc with std in /amd64/rust/lib/rustlib, and 9front-gcc's native gcc
# (/amd64/gnu, the linker) in the guest; compiles and runs tests/std1.rs there.
# rustc is called as /bin/rustc (bind), its sysroot /amd64/rust (rustc's fallback on plan9).
# Prints "name ok" or "name FAIL ...". Log: logs/rustc-guest.log.
# env: GCC9OUT (the native toolchain, default ../9front-gcc/out), MEM (guest MB, 3072),
# PLAN9 (the system booted: its APE and libp9.a link the program; default ../9front-gccwt)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
export PLAN9=${PLAN9:-$TREE}
GCC9=${GCC9:-$GCC_DIR}
GCC9OUT=${GCC9OUT:-$GCC9/out}
GCC9BUILD=${GCC9BUILD:-$GCC9/build}
# objtype=386: i686-unknown-plan9's rustc in a 9pc guest (logs/rustc-guest.386.log)
case ${objtype:=amd64} in
amd64)	TG=x86_64-unknown-plan9 XT=x86_64-pc-plan9 L= ;;
386)	TG=i686-unknown-plan9 XT=i686-pc-plan9 L=.386 ;;
arm64)	TG=aarch64-unknown-plan9 XT=aarch64-pc-plan9 L=.arm64 ;;
*)	echo "$0: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
A=$objtype
X=$GCC9BUILD/$A/cross/bin/$XT
R=$TOP/rust/build
I=$TOP/build/rustc-plan9$L
rm -rf "$I"
mkdir -p "$I/bin" "$I/lib/rustlib/$TG/lib"
"$X-strip" -o "$I/bin/rustc" "$R/$TG/stage2/bin/rustc"
cp "$R"/x86_64-unknown-linux-gnu/stage1/lib/rustlib/$TG/lib/*.r{lib,meta} \
	"$I/lib/rustlib/$TG/lib/"
# rustc, the native gcc, and what it links with, on an ext4 disk (tests/guest.sh -D: the
# guest's root is the host's over 9P, slow for these); the root has no headers and archives
args=(-D "/$A/rust=$I" -D "/$A/gnu=$GCC9OUT/$A/$A/gnu"
	-D "/$A/include=$PLAN9/$A/include" -D "/$A/lib.gcc=$PLAN9/$A/lib.gcc"
	-x "/$A/plan9.ld=$PLAN9/$A/plan9.ld" -D "/sys/include=$PLAN9/sys/include")
cmds=(
	"bind -b /$A/gnu/bin /bin; bind -b /$A/rust/bin /bin"
	"rustc --version; if(~ \$status '') echo version ok; if not echo version FAIL \$status"
	"cd /tmp; rustc --edition 2024 -O /tests/std1.rs -o std1 >[2=1] | sed 20q; if(test -x std1) echo compile ok; if not echo compile FAIL"
	"/tmp/std1 'a b' ü; if(~ \$status '') echo run ok; if not echo run FAIL \$status"
)
MEM=${MEM:-3072} CMDWAIT=${CMDWAIT:-1800} GUEST_TIMEOUT=${GUEST_TIMEOUT:-3000} "$TOP/tests/guest.sh" "${args[@]}" -f "$TOP/tests/std1.rs" -- "${cmds[@]}" \
	> "$TOP/logs/rustc-guest$L.log" 2>&1
grep -E '^(version|compile|run) (ok|FAIL)' "$TOP/logs/rustc-guest$L.log" || { tail -30 "$TOP/logs/rustc-guest$L.log"; exit 1; }

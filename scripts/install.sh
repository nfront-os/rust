#!/bin/bash
# The Rust toolchain for an installed 9front system, an optional install: the tree
# out/$objtype/$objtype/rust that goes to /$objtype/rust (as 9front-gcc's
# out/$objtype/$objtype/gnu goes to /$objtype/gnu), and two archives of it:
#   out/$objtype/rust-T.tgz        rustc, cargo, std (bin/rustc, bin/cargo, lib, README)
#   out/$objtype/rust-tools-T.tgz  rustdoc, rustfmt, cargo-fmt, clippy-driver, cargo-clippy
# Paths in them from /, so on 9front:  cd /; tar xzf rust-T.tgz  (9front's tar).
# ../9front-gccwt's scripts/guest/stage-dist.sh puts the tree on the install CD only if
# asked (RUST_OUT=out/$objtype).
#   bin/rustc               the stage-2 rustc for 9front (static, LLVM inside)
#   bin/cargo               scripts/build-cargo.sh's
#   bin/rustdoc, rustfmt, cargo-fmt, clippy-driver, cargo-clippy   if built (x build
#                           --stage 2 --host T --target T src/tools/...; NOTES Teil 13)
#   lib/rustlib/T/lib       std and its crates for T (rlib and rmeta)
#   README                  how to use it
# All stripped. rustc finds its sysroot from its own path (/$objtype/rust); it links with
# 9front-gcc's gcc (/$objtype/gnu), which must be installed too.
# usage: [objtype=386|arm64] scripts/install.sh [outdir]   (default out/$objtype)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
GCC9BUILD=${GCC9BUILD:-$GCC_DIR/build}
case ${objtype:=amd64} in
amd64)	T=x86_64-unknown-plan9 X=x86_64-pc-plan9 ;;
386)	T=i686-unknown-plan9 X=i686-pc-plan9 ;;
arm64)	T=aarch64-unknown-plan9 X=aarch64-pc-plan9 ;;
*)	echo "install.sh: objtype=$objtype: no target" >&2; exit 1 ;;
esac
A=$objtype
OUT=${1:-$TOP/out/$A}
R=$TOP/rust/build
D=$OUT/$A/rust
STRIP=$GCC9BUILD/$A/cross/bin/$X-strip
die() { echo "install.sh: $*" >&2; exit 1; }

rustc=$R/$T/stage2/bin/rustc
cargo=$TOP/tests/out/cargo-port/$T/release/cargo
# std from the stage-1 sysroot: the stage-2 one is the same files (bootstrap uplifts it),
# but a stage-2 build for one target removes the others'
std=$R/x86_64-unknown-linux-gnu/stage1/lib/rustlib/$T/lib
[ -x "$rustc" ] || die "no $rustc: ./x build --stage 2 --host $T --target $T compiler/rustc"
[ -x "$cargo" ] || die "no $cargo: objtype=$A scripts/build-cargo.sh"
ls "$std"/libstd-*.rlib >/dev/null 2>&1 || die "no std in $std: ./x build --stage 1 library --target $T"

rm -rf "$D"
mkdir -p "$D/bin" "$D/lib/rustlib/$T/lib"
"$STRIP" -o "$D/bin/rustc" "$rustc"
"$STRIP" -o "$D/bin/cargo" "$cargo"
# the tools, where bootstrap builds them for the host T (stage2/bin it makes anew each run)
TOOLS=$R/x86_64-unknown-linux-gnu/stage2-tools/$T/release
for t in rustdoc:rustdoc-tool-binary rustfmt cargo-fmt clippy-driver cargo-clippy; do
	f=$TOOLS/${t#*:}
	if [ -x "$f" ]; then
		"$STRIP" -o "$D/bin/${t%%:*}" "$f"
	else
		echo "install.sh: no ${t%%:*} ($f not built)" >&2
	fi
done
# rlibs and rmeta: bootstrap builds std with -Zembed-metadata=no, its metadata only in
# the .rmeta beside each rlib. Only std's, as its stamp lists them: after a build of the
# tools for the host T the sysroot also has the compiler's crates (rustc_private)
stamp=$R/x86_64-unknown-linux-gnu/stage1-std/$T/dist/.libstd-stamp
[ -s "$stamp" ] || die "no $stamp: ./x build --stage 1 library --target $T"
tr '\0' '\n' < "$stamp" | sed -n 's|^t.*/||p' | grep -E '\.(rlib|rmeta)$' |
	while read -r f; do cp "$std/$f" "$D/lib/rustlib/$T/lib/"; done
version=$(cd "$TOP/rust" && git log -1 --format='%h %cd' --date=short)
sed "s/@objtype@/$A/g; s/@target@/$T/g; s/@version@/$version/" > "$D/README" <<'EOF'
Rust for 9front (@target@), from 9front-rust (rust @version@)

	bind -b /@objtype@/gnu/bin /bin		# gcc: rustc links with it
	bind -b /@objtype@/rust/bin /bin
	cargo new hello && cd hello && cargo run

bin/	rustc, cargo, and the tools that were built (rustdoc, rustfmt,
	cargo-fmt, clippy-driver, cargo-clippy)
lib/rustlib/@target@/lib	std and its crates

rustc finds this tree from its own path (/@objtype@/rust). Programs link
with 9front-gcc's gcc against /@objtype@/lib.gcc (libp9.a, APE) and
/@objtype@/plan9.ld; std uses 9front's own libc (not APE). cargo reaches
crates.io over https: it needs the CA roots in /sys/lib/tls/ca.pem
(9front ships none). Proc macros run as programs (no dylibs on Plan 9).
EOF
# the archives: ustar, owner sys (9front's tar takes the names), every directory listed
tools=" rustdoc rustfmt cargo-fmt clippy-driver cargo-clippy "
base=() extra=("$A" "$A/rust" "$A/rust/bin")
while read -r f; do
	t=${f#$A/rust/bin/}
	if [ "$t" != "$f" ] && [[ $tools == *" $t "* ]]; then extra+=("$f"); else base+=("$f"); fi
done < <(cd "$OUT" && { echo "$A"; find "$A/rust" | sort; })
tarit() {
	tar -C "$OUT" --format=ustar --owner=sys --group=sys --mode='u+rwX,go+rX,go-w' \
		--no-recursion -czf "$1" "${@:2}"
}
tarit "$OUT/rust-$T.tgz" "${base[@]}"
[ ${#extra[@]} = 3 ] || tarit "$OUT/rust-tools-$T.tgz" "${extra[@]}"
du -sh "$D"
ls -l "$D/bin" | awk 'NR > 1 {print $5, $9}'
ls -l "$OUT"/rust-*"$T".tgz | awk '{print $5, $9}'

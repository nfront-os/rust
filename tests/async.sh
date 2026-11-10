#!/bin/bash
# async on 9front: mio (crates/mio-1.2.1, sys::plan9 over std's non-blocking sockets) and
# tokio (crates/tokio-1.54.1) cross-built with the stage-1 rustc, run in the guest:
# tests/crates/mioecho (one poll loop, an echo, a waker), tests/crates/tokioecho (a
# multi-thread runtime: 20 echo clients with half close, sleep, timeout, a channel, fs),
# tests/crates/httptest (hyper's HTTP/1.1 and HTTP/2 server and clients, hyper-util's client,
# reqwest), tests/crates/tlsdns (DNS through /net/cs, HTTPS with rustls here and to crates.io:
# needs the network; the host's roots as /sys/lib/tls/ca.pem), tests/crates/tokioproc
# (tokio::process, tokio::signal over notes; in a note group of its own: it posts to it).
# Needs the 9front tree with TCP's half close (gcc branch: ctl "shutdown").
# Prints "name ok" or "name FAIL ...". Log: logs/async-guest[.386].log.
# objtype=386: i686-unknown-plan9 in a 9pc guest; objtype=arm64: aarch64-unknown-plan9 (qemu);
# objtype=riscv64: riscv64gc-unknown-plan9 in the qemu virt guest of ../9front-riscv (PLAN9_RISCV).
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
export PLAN9=${PLAN9:-$TOP/../9front-gccwt}
GCC9BUILD=${GCC9BUILD:-$TOP/../9front-gcc/build-gcc}
case ${objtype:=amd64} in
amd64)	T=x86_64-unknown-plan9 X=x86_64-pc-plan9 L= ;;
386)	T=i686-unknown-plan9 X=i686-pc-plan9 L=.386 ;;
arm64)	T=aarch64-unknown-plan9 X=aarch64-pc-plan9 L=.arm64 ;;
riscv64)	T=riscv64gc-unknown-plan9 X=riscv64-unknown-plan9 L=.riscv64
	PLAN9=${PLAN9_RISCV:-$TOP/../9front-riscv} ;;
*)	echo "async.sh: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
export PATH=$GCC9BUILD/$objtype/cross/bin:$PATH
export RUSTC=$TOP/rust/build/host/stage1/bin/rustc
TV=$(echo $T | tr a-z- A-Z_)
export CARGO_TARGET_${TV}_LINKER=$X-gcc
mkdir -p "$TOP/logs"
bins=()
for c in mioecho tokioecho httptest tlsdns tokioproc; do
	cargo build --release --target $T --manifest-path "$TOP/tests/crates/$c/Cargo.toml" \
		--config "$TOP/cargo-plan9.toml" > "$TOP/logs/async-build-$c$L.log" 2>&1 ||
		{ tail -20 "$TOP/logs/async-build-$c$L.log"; exit 1; }
	bins+=("$TOP/tests/crates/$c/target/$T/release/$c")
done
run() { echo "$2 >[2=1]; if(~ \$status '') echo $1 ok; if not echo $1 FAIL \$status"; }
"$TOP/tests/guest.sh" -x /sys/lib/tls/ca.pem=/etc/ssl/certs/ca-certificates.crt -f "${bins[@]}" -- "ip/ipconfig loopback /dev/null 127.1" \
	"$(run mio /tests/mioecho)" "$(run tokio /tests/tokioecho)" "$(run http /tests/httptest)" "$(run tlsdns /tests/tlsdns)" \
	"$(run proc '@{rfork s; exec /tests/tokioproc}')" > "$TOP/logs/async-guest$L.log" 2>&1
grep -E '^(mio|tokio|http|tlsdns|proc) (ok|FAIL)' "$TOP/logs/async-guest$L.log" || { tail -20 "$TOP/logs/async-guest$L.log"; exit 1; }
[ "$(grep -cE '^(mio|tokio|http|tlsdns|proc) ok' "$TOP/logs/async-guest$L.log")" = 5 ]

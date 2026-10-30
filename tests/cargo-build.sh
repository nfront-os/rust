#!/bin/bash
# cargo build on 9front with rustc on 9front (tests/rustc.sh's install, the native gcc):
# a new package built and run, a crates.io dependency (itoa), one with a build script
# (serde without derive: the build script built and run on 9front), and --release.
# Needs the network (crates.io); the guest's CA roots are the host's.
# Prints "name ok" or "name FAIL ...". Log: logs/cargo-build-guest.log.
# env: as tests/rustc.sh (GCC9OUT, PLAN9, MEM), SMP (guest processors, 4)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
export PLAN9=${PLAN9:-$TOP/../9front-gccwt}
GCC9=${GCC9:-$TOP/../9front-gcc}
GCC9OUT=${GCC9OUT:-$GCC9/out-gcc}
GCC9BUILD=${GCC9BUILD:-$GCC9/build-gcc}
X=$GCC9BUILD/amd64/cross/bin/x86_64-pc-plan9
R=$TOP/rust/build
I=$TOP/build/rustc-plan9
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
rm -rf "$I"
mkdir -p "$I/bin" "$I/lib/rustlib/x86_64-unknown-plan9/lib"
"$X-strip" -o "$I/bin/rustc" "$R/x86_64-unknown-plan9/stage2/bin/rustc"
"$X-strip" -o "$I/bin/cargo" "$TOP/tests/out/cargo-port/x86_64-unknown-plan9/release/cargo"
cp "$R"/x86_64-unknown-linux-gnu/stage2/lib/rustlib/x86_64-unknown-plan9/lib/*.r{lib,meta} \
	"$I/lib/rustlib/x86_64-unknown-plan9/lib/"
cat > "$T/t.rc" <<'RC'
#!/bin/rc
fn t { if(~ $status '') echo $1 ok; if not echo $1 FAIL $status }
bind -b /amd64/rust/bin /bin
bind -b /amd64/gnu/bin /bin
# rustc finds its sysroot from its own path: /bin/rustc (the bind) would be /
RUSTC=/amd64/rust/bin/rustc
cd /tmp
cargo new hello >[2]/dev/null
cd hello
RUST_BACKTRACE=1 cargo build >[2=1] | sed 60q
t build
./target/debug/hello | grep -s '^Hello, world!$'
t run
echo 'itoa = "1"' >> Cargo.toml
cat > src/main.rs <<'EOF2'
fn main() {
    let mut b = itoa::Buffer::new();
    println!("itoa: {}", b.format(-12345i32));
}
EOF2
cargo build >[2=1] | tail -5
t dep
./target/debug/hello | grep -s '^itoa: -12345$'
t deprun
echo 'serde = "1"' >> Cargo.toml
cargo build -v >[2=1] | grep -E 'build-script|error|warning: unused' | sed 10q
cargo build >[2=1] | tail -3
t buildscript
cargo build --release >[2=1] | tail -3
./target/release/hello | grep -s '^itoa: -12345$'
t release
RC
cd "$TOP"
SMP=${SMP:-4} MEM=${MEM:-3072} CMDWAIT=${CMDWAIT:-3300} GUEST_TIMEOUT=${GUEST_TIMEOUT:-3600} tests/guest.sh \
	-D "/amd64/rust=$I" -D "/amd64/gnu=$GCC9OUT/amd64/amd64/gnu" \
	-D "/amd64/include=$PLAN9/amd64/include" -D "/amd64/lib.gcc=$PLAN9/amd64/lib.gcc" \
	-x "/amd64/plan9.ld=$PLAN9/amd64/plan9.ld" -D "/sys/include=$PLAN9/sys/include" \
	-x /sys/lib/tls/ca.pem=/etc/ssl/certs/ca-certificates.crt \
	-f "$T/t.rc" -- "rc /tests/t.rc" > logs/cargo-build-guest.log 2>&1
tests="build run dep deprun buildscript release"
grep -E "^(${tests// /|}) (ok|FAIL)" logs/cargo-build-guest.log || { tail -30 logs/cargo-build-guest.log; exit 1; }
[ "$(grep -cE "^(${tests// /|}) ok" logs/cargo-build-guest.log)" = 6 ]

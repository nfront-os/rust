#!/bin/bash
# cargo on 9front (scripts/build-cargo.sh's) in the guest: --version, new (libgit2),
# search (crates.io's API over curl and rustls), fetch (the index, downloads, unpacking,
# sqlite). 9front has no rustc yet: a stand-in rc script answers cargo's questions with the
# stage-1 rustc's answers for x86_64-unknown-plan9. The guest's CA roots are the host's
# (9front has none: /sys/lib/tls/ca.pem). Needs the network. Log: logs/cargo-guest.log.
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
GCC9=${GCC9:-$TOP/../9front-gcc}
# its build directory with the toolchain over gcc's APE (9front branch gcc)
GCC9BUILD=${GCC9BUILD:-$GCC9/build-gcc}
R=$TOP/rust/build/host/stage1/bin/rustc
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
"$GCC9BUILD/amd64/cross/bin/x86_64-pc-plan9-strip" -o "$T/cargo" "$TOP/tests/out/cargo-port/x86_64-unknown-plan9/release/cargo"
"$R" -vV | sed 's/^host: .*/host: x86_64-unknown-plan9/' > "$T/rustc.vV"
"$R" - --crate-name ___ --print=file-names --target x86_64-unknown-plan9 --crate-type bin \
	--crate-type rlib --crate-type dylib --crate-type cdylib --crate-type staticlib \
	--crate-type proc-macro --print=sysroot --print=split-debuginfo --print=crate-name \
	--print=cfg -Wwarnings < /dev/null > "$T/rustc.cfg" 2> "$T/rustc.err"
printf '#!/bin/rc\n# a stand-in of rustc: its answers about x86_64-unknown-plan9\nif(~ $1 -vV){\n\tcat /tests/rustc.vV\n\texit\n}\ncat /tests/rustc.err >[1=2]\ncat /tests/rustc.cfg\n' > "$T/rustc"
chmod +x "$T/rustc"
cat > "$T/t.rc" <<'RC'
#!/bin/rc
fn t { if(~ $status '') echo $1 ok; if not echo $1 FAIL $status }
/tests/cargo --version
t version
cd /tmp
/tests/cargo new hello >[2]/dev/null
test -d hello/.git && test -f hello/src/main.rs
t new
/tests/cargo search serde --limit 1 | grep -s '^serde = '
t search
cd hello
echo 'itoa = "1"' >> Cargo.toml
echo 'serde = { version = "1", features = ["derive"] }' >> Cargo.toml
RUSTC=/tests/rustc /tests/cargo fetch && test -f $home/.cargo/registry/src/*/itoa-1*/src/lib.rs
t fetch
RUSTC=/tests/rustc /tests/cargo fetch >[2=1] | grep -s Downloaded
if(~ $status '') echo refetch FAIL downloaded again
if not echo refetch ok
RC
cd "$TOP"
GUEST_TIMEOUT=${GUEST_TIMEOUT:-1200} tests/guest.sh -x /sys/lib/tls/ca.pem=/etc/ssl/certs/ca-certificates.crt \
	-f "$T/cargo" "$T/rustc" "$T/rustc.vV" "$T/rustc.cfg" "$T/rustc.err" "$T/t.rc" -- "rc /tests/t.rc" > logs/cargo-guest.log 2>&1
tests="version new search fetch refetch"
grep -E "^(${tests// /|}) (ok|FAIL)" logs/cargo-guest.log
[ "$(grep -cE "^(${tests// /|}) ok" logs/cargo-guest.log)" = 5 ]

#!/bin/bash
# cargo on 9front (scripts/build-cargo.sh's) in the guest: --version, new (libgit2),
# search (crates.io's API over curl and rustls), fetch (the index, downloads, unpacking,
# sqlite). 9front has no rustc yet: a stand-in rc script answers cargo's questions with the
# stage-1 rustc's answers for the target. The guest's CA roots are the host's
# (9front has none: /sys/lib/tls/ca.pem). Needs the network. Log: logs/cargo-guest.log.
# objtype=386: i686-unknown-plan9's cargo in a 9pc guest (log cargo-guest.386.log).
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
GCC9=${GCC9:-$GCC_DIR}
# its build directory with the toolchain over gcc's APE (9front branch gcc)
GCC9BUILD=${GCC9BUILD:-$GCC9/build}
case ${objtype:=amd64} in
amd64)	TG=x86_64-unknown-plan9 X=x86_64-pc-plan9 LOG=cargo-guest.log ;;
386)	TG=i686-unknown-plan9 X=i686-pc-plan9 LOG=cargo-guest.386.log
	export PLAN9=${PLAN9:-$TREE_GCC} ;;
arm64)	TG=aarch64-unknown-plan9 X=aarch64-pc-plan9 LOG=cargo-guest.arm64.log
	export PLAN9=${PLAN9:-$TREE_GCC} ;;
riscv64)	TG=riscv64gc-unknown-plan9 X=riscv64-unknown-plan9 LOG=cargo-guest.riscv64.log
	export PLAN9=${PLAN9:-$TREE_RISCV} ;;
*)	echo "cargo.sh: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
R=$TOP/rust/build/host/stage1/bin/rustc
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
"$GCC9BUILD/$objtype/cross/bin/$X-strip" -o "$T/cargo" "$TOP/tests/out/cargo-port/$TG/release/cargo"
"$R" -vV | sed "s/^host: .*/host: $TG/" > "$T/rustc.vV"
"$R" - --crate-name ___ --print=file-names --target $TG --crate-type bin \
	--crate-type rlib --crate-type dylib --crate-type cdylib --crate-type staticlib \
	--crate-type proc-macro --print=sysroot --print=split-debuginfo --print=crate-name \
	--print=cfg -Wwarnings < /dev/null > "$T/rustc.cfg" 2> "$T/rustc.err"
printf '#!/bin/rc\n# a stand-in of rustc: its answers about '"$TG"'\nif(~ $1 -vV){\n\tcat /tests/rustc.vV\n\texit\n}\ncat /tests/rustc.err >[1=2]\ncat /tests/rustc.cfg\n' > "$T/rustc"
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
	-f "$T/cargo" "$T/rustc" "$T/rustc.vV" "$T/rustc.cfg" "$T/rustc.err" "$T/t.rc" -- "rc /tests/t.rc" > logs/$LOG 2>&1
tests="version new search fetch refetch"
grep -E "^(${tests// /|}) (ok|FAIL)" logs/$LOG
[ "$(grep -cE "^(${tests// /|}) ok" logs/$LOG)" = 5 ]

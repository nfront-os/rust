#!/bin/bash
# Rust installed on 9front from scripts/install.sh's archives (rust-T.tgz, rust-tools-T.tgz),
# unpacked with 9front's tar in the guest as /$objtype/rust (with 9front-gcc's gcc as
# /$objtype/gnu, the linker), then used as a user would: rustc, cargo,
# rustdoc, rustfmt and clippy on a new crate. cargo build and run; cargo fmt (--check fails
# on badly formatted code, passes after cargo fmt); cargo clippy (a lint it must report);
# cargo doc (the HTML); cargo test with a doc test (rustdoc compiles and runs it on 9front).
# Prints "name ok" or "name FAIL ...". Log: logs/tools-guest[.386].log.
# env: as tests/rustc.sh (GCC9OUT, PLAN9, MEM); OUT: install.sh's output (out/$objtype).
# objtype=386: i686-unknown-plan9 in a 9pc guest.
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
export PLAN9=${PLAN9:-$TREE}
GCC9OUT=${GCC9OUT:-$GCC_DIR/out}
case ${objtype:=amd64} in
amd64)	L= ;;
386)	L=.386 ;;
arm64)	L=.arm64 ;;
*)	echo "$0: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
A=$objtype
OUT=${OUT:-$TOP/out/$A}
case $A in amd64) TG=x86_64-unknown-plan9 ;; 386) TG=i686-unknown-plan9 ;; arm64) TG=aarch64-unknown-plan9 ;; esac
for f in rust-$TG.tgz rust-tools-$TG.tgz; do
	[ -s "$OUT/$f" ] || { echo "$0: no $OUT/$f: objtype=$A scripts/install.sh" >&2; exit 1; }
done
T=$(mktemp -d -p "$TOP/build")	# beside out/: tests/guest.sh -D links from it
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/arch" "$T/empty"
ln "$OUT/rust-$TG.tgz" "$OUT/rust-tools-$TG.tgz" "$T/arch/" 2>/dev/null ||
	cp "$OUT/rust-$TG.tgz" "$OUT/rust-tools-$TG.tgz" "$T/arch/"
cat > "$T/t.rc" <<EOF
#!/bin/rc
fn t { if(~ \$status '') echo \$1 ok; if not echo \$1 FAIL \$status }
fn tnot { if(~ \$status '') echo \$1 FAIL 'no error'; if not echo \$1 ok }
# tg name pattern file: the pattern in the file, else its start
fn tg { if(grep -s \$2 \$3) echo \$1 ok; if not { echo \$1 FAIL; sed 30q \$3 } }
# the optional install: the archives unpacked from /, here into a root of their own on the
# disk (the guest's / is the host's, read-only for this), bound where they would be
mkdir -p /tmp/root
cd /tmp/root
tar xzf /n/arch/rust-$TG.tgz
t untar-base
tar xzf /n/arch/rust-tools-$TG.tgz
t untar-tools
bind /tmp/root/$A/rust /$A/rust
bind -b /$A/gnu/bin /bin
bind -b /$A/rust/bin /bin
for(p in rustc cargo rustdoc rustfmt cargo-fmt clippy-driver cargo-clippy){
	\$p --version >[2=1] | sed 1q
	t version-\$p
}
cd /tmp
cargo new --lib demo >[2]/dev/null
cd demo
cat > src/lib.rs <<'EOR'
/// Adds two numbers.
///
/// \`\`\`
/// assert_eq!(demo::add(2, 3), 5);
/// \`\`\`
pub fn add(a: i32, b: i32) -> i32 {
       return a+b;
}

#[cfg(test)]
mod tests {
    #[test]
    fn adds() { assert_eq!(super::add(1, 1), 2); }
}
EOR
mkdir -p src/bin
cat > src/bin/hello.rs <<'EOR'
fn main() {
    println!("hello {}", demo::add(40, 2));
}
EOR
cargo build >[2=1] | tail -3
t build
cargo run -q --bin hello >[2=1] | grep -s '^hello 42$'
t run
cargo fmt --check > /dev/null >[2=1]
tnot fmt-check-unformatted
cargo fmt >[2=1] | tail -3
t fmt
cargo fmt --check >[2=1] | tail -3
t fmt-check-formatted
tg fmt-result '    return a \+ b;' src/lib.rs
cargo clippy > /tmp/clippy.log >[2=1]
t clippy
tg clippy-lint needless_return /tmp/clippy.log
cargo doc >[2=1] | tail -3
test -s target/doc/demo/index.html && test -s target/doc/demo/fn.add.html
t doc
cargo test > /tmp/test.log >[2=1]
t test
tg test-unit 'test tests::adds ... ok' /tmp/test.log
tg test-doc 'src/lib.rs - add .line 3. ... ok' /tmp/test.log
EOF
args=(-D "/n/arch=$T/arch" -d "/$A/rust=$T/empty" -D "/$A/gnu=$GCC9OUT/$A/$A/gnu"
	-D "/$A/include=$PLAN9/$A/include" -D "/$A/lib.gcc=$PLAN9/$A/lib.gcc"
	-x "/$A/plan9.ld=$PLAN9/$A/plan9.ld" -D "/sys/include=$PLAN9/sys/include")
mkdir -p "$TOP/logs"
MEM=${MEM:-3072} CMDWAIT=${CMDWAIT:-1800} GUEST_TIMEOUT=${GUEST_TIMEOUT:-3000} "$TOP/tests/guest.sh" "${args[@]}" \
	-f "$T/t.rc" -- "rc /tests/t.rc" > "$TOP/logs/tools-guest$L.log" 2>&1
tests="untar-base untar-tools version-rustc version-cargo version-rustdoc version-rustfmt version-cargo-fmt version-clippy-driver
	version-cargo-clippy build run fmt-check-unformatted fmt fmt-check-formatted fmt-result clippy
	clippy-lint doc test test-unit test-doc"
tests=$(echo $tests)
grep -E "^(${tests// /|}) (ok|FAIL)" "$TOP/logs/tools-guest$L.log" || { tail -30 "$TOP/logs/tools-guest$L.log"; exit 1; }
[ "$(grep -cE "^(${tests// /|}) ok" "$TOP/logs/tools-guest$L.log")" = "$(echo $tests | wc -w)" ]

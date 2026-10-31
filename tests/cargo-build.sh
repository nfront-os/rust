#!/bin/bash
# cargo build on 9front with rustc on 9front (tests/rustc.sh's install, the native gcc):
# a new package built and run, a crates.io dependency (itoa), one with a build script
# (serde without derive: the build script built and run on 9front), --release, and proc
# macros (programs on 9front): derive, attribute and bang of our own, nested ones
# (expand_expr), failing inner ones, serde's derive.
# Needs the network (crates.io); the guest's CA roots are the host's.
# Prints "name ok" or "name FAIL ...". Log: logs/cargo-build-guest.log.
# env: as tests/rustc.sh (GCC9OUT, PLAN9, MEM), SMP (guest processors, 4)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
export PLAN9=${PLAN9:-$TOP/../9front-gccwt}
GCC9=${GCC9:-$TOP/../9front-gcc}
GCC9OUT=${GCC9OUT:-$GCC9/out-gcc}
GCC9BUILD=${GCC9BUILD:-$GCC9/build-gcc}
# objtype=386: i686-unknown-plan9's rustc in a 9pc guest (logs/cargo-build-guest.386.log)
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
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
rm -rf "$I"
mkdir -p "$I/bin" "$I/lib/rustlib/$TG/lib"
"$X-strip" -o "$I/bin/rustc" "$R/$TG/stage2/bin/rustc"
"$X-strip" -o "$I/bin/cargo" "$TOP/tests/out/cargo-port/$TG/release/cargo"
cp "$R"/x86_64-unknown-linux-gnu/stage1/lib/rustlib/$TG/lib/*.r{lib,meta} \
	"$I/lib/rustlib/$TG/lib/"
cat > "$T/t.rc" <<'RC'
#!/bin/rc
fn t { if(~ $status '') echo $1 ok; if not echo $1 FAIL $status }
bind -b /amd64/rust/bin /bin
bind -b /amd64/gnu/bin /bin
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
cp ./target/debug/hello /tmp/hello-dep
echo 'serde = "1"' >> Cargo.toml
cargo build -v >[2=1] | grep 'build-script|error|warning: unused' | sed 10q
cargo build >[2=1] | tail -3
t buildscript
cargo build --release >[2=1] | tail -3
./target/release/hello | grep -s '^itoa: -12345$'
t release
# proc macros (programs on 9front): derive, attribute and bang from a crate of our own
cd /tmp
cargo new --lib hellomacro >[2]/dev/null
cd hellomacro
echo '[lib]' >> Cargo.toml
echo 'proc-macro = true' >> Cargo.toml
cat > src/lib.rs <<'EOF2'
use proc_macro::TokenStream;

#[proc_macro_derive(Hello)]
pub fn hello(input: TokenStream) -> TokenStream {
    let s = input.to_string();
    let name = s.split_whitespace().skip_while(|w| *w != "struct").nth(1).unwrap();
    let name = name.trim_end_matches(';');
    format!("impl {name} {{ pub fn hello() -> &'static str {{ \"hello from {name}\" }} }}")
        .parse()
        .unwrap()
}

#[proc_macro_attribute]
pub fn keep(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro]
pub fn answer(_input: TokenStream) -> TokenStream {
    "42".parse().unwrap()
}
EOF2
cd /tmp
cargo new pmapp >[2]/dev/null
cd pmapp
echo 'hellomacro = { path = "../hellomacro" }' >> Cargo.toml
cat > src/main.rs <<'EOF2'
use hellomacro::{Hello, answer, keep};

#[derive(Hello)]
struct Thing;

#[keep]
fn n() -> i32 {
    answer!()
}

fn main() {
    println!("{} {}", Thing::hello(), n());
}
EOF2
cargo build >[2=1] | tail -5
./target/debug/pmapp | grep -s '^hello from Thing 42$'
t procmacro
# nested proc macros: expand_expr of a macro of the same crate runs it in another instance of
# the program while this one waits (nest!: four at once); a builtin macro through expand_expr
cd /tmp
cargo new --lib nestmacro >[2]/dev/null
cd nestmacro
echo '[lib]' >> Cargo.toml
echo 'proc-macro = true' >> Cargo.toml
cat > src/lib.rs <<'EOF2'
#![feature(proc_macro_expand)]
use proc_macro::TokenStream;

#[proc_macro]
pub fn answer(_input: TokenStream) -> TokenStream {
    "42".parse().unwrap()
}

#[proc_macro]
pub fn twice(input: TokenStream) -> TokenStream {
    let e = input.expand_expr().expect("expand_expr");
    let n: i64 = e.to_string().parse().expect("a number");
    (n * 2).to_string().parse().unwrap()
}

#[proc_macro]
pub fn nest(_input: TokenStream) -> TokenStream {
    let t: TokenStream =
        "::nestmacro::twice!(::nestmacro::twice!(::nestmacro::answer!()))".parse().unwrap();
    t.expand_expr().expect("expand_expr")
}

#[proc_macro]
pub fn shout(input: TokenStream) -> TokenStream {
    let e = input.expand_expr().expect("expand_expr");
    let s = e.to_string();
    format!("{:?}", s.trim_matches('"').to_uppercase()).parse().unwrap()
}
EOF2
cd /tmp
cargo new nestapp >[2]/dev/null
cd nestapp
echo 'nestmacro = { path = "../nestmacro" }' >> Cargo.toml
cat > src/main.rs <<'EOF2'
use nestmacro::{answer, nest, shout, twice};

fn main() {
    println!("{} {} {} {}", answer!(), twice!(answer!()), nest!(), shout!(concat!("plan", 9)));
}
EOF2
cargo build >[2=1] | tail -8
./target/debug/nestapp | grep -s '^42 84 168 PLAN9$'
t expandexpr
# a macro that fails in an inner level: a panic (as on Linux: one error, the program goes
# on) and an exit (its instance is gone: an error, the next run gets a new one); the macros
# after them still expand (the asserts hold)
cd /tmp
cargo new --lib crashmacro >[2]/dev/null
cd crashmacro
echo '[lib]' >> Cargo.toml
echo 'proc-macro = true' >> Cargo.toml
cat > src/lib.rs <<'EOF2'
#![feature(proc_macro_expand)]
use proc_macro::TokenStream;

#[proc_macro]
pub fn answer(_input: TokenStream) -> TokenStream {
    "42".parse().unwrap()
}

// the expansion of its input times two; 0 if that expansion fails (rustc reports it)
#[proc_macro]
pub fn twice(input: TokenStream) -> TokenStream {
    match input.expand_expr() {
        Ok(e) => (e.to_string().parse::<i64>().expect("a number") * 2).to_string().parse().unwrap(),
        Err(_) => "0".parse().unwrap(),
    }
}

#[proc_macro]
pub fn nest(_input: TokenStream) -> TokenStream {
    "::crashmacro::twice!(::crashmacro::twice!(::crashmacro::answer!()))"
        .parse::<TokenStream>()
        .unwrap()
        .expand_expr()
        .expect("expand_expr")
}

#[proc_macro]
pub fn boom(_input: TokenStream) -> TokenStream {
    panic!("boom inside")
}

#[proc_macro]
pub fn die(_input: TokenStream) -> TokenStream {
    std::process::exit(3)
}
EOF2
cd /tmp
cargo new crashapp >[2]/dev/null
cd crashapp
echo 'crashmacro = { path = "../crashmacro" }' >> Cargo.toml
cat > src/main.rs <<'EOF2'
use crashmacro::{answer, boom, die, nest, twice};

const A: i64 = twice!(boom!());
const B: i64 = twice!(die!());
const C: () = assert!(twice!(answer!()) == 84);
const D: () = assert!(nest!() == 168);

fn main() {
    println!("{A} {B}");
}
EOF2
cargo build >/tmp/crash.log >[2=1]
if(grep -s 'message: boom inside' /tmp/crash.log && grep -s 'message: proc-macro program failed' /tmp/crash.log && ~ `{grep -c '^error: proc macro panicked' /tmp/crash.log} 2 && ! grep -s 'assert|evaluat|^error\[' /tmp/crash.log) test 1 -eq 1
if not { sed 60q /tmp/crash.log; test 1 -eq 0 }
t macrocrash
# serde's derive (syn, quote, proc-macro2) and serde_json
cd /tmp
cargo new sd >[2]/dev/null
cd sd
echo 'serde = { version = "1", features = ["derive"] }' >> Cargo.toml
echo 'serde_json = "1"' >> Cargo.toml
cat > src/main.rs <<'EOF2'
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Point {
    x: i32,
    name: String,
}

fn main() {
    let p = Point { x: 1, name: "plan9".into() };
    let s = serde_json::to_string(&p).unwrap();
    let q: Point = serde_json::from_str(&s).unwrap();
    assert_eq!(p, q);
    println!("{s}");
}
EOF2
cargo build >[2=1] | tail -5
./target/debug/sd | grep -s '^{"x":1,"name":"plan9"}$'
t serdederive
RC
# the guest's paths are its objtype's
sed -i "s|/amd64/|/$A/|g" "$T/t.rc"
cd "$TOP"
SMP=${SMP:-4} MEM=${MEM:-3072} CMDWAIT=${CMDWAIT:-3300} GUEST_TIMEOUT=${GUEST_TIMEOUT:-3600} tests/guest.sh \
	-D "/$A/rust=$I" -D "/$A/gnu=$GCC9OUT/$A/$A/gnu" \
	-D "/$A/include=$PLAN9/$A/include" -D "/$A/lib.gcc=$PLAN9/$A/lib.gcc" \
	-x "/$A/plan9.ld=$PLAN9/$A/plan9.ld" -D "/sys/include=$PLAN9/sys/include" \
	-x /sys/lib/tls/ca.pem=/etc/ssl/certs/ca-certificates.crt \
	-f "$T/t.rc" -- "rc /tests/t.rc" > logs/cargo-build-guest$L.log 2>&1
tests="build run dep deprun buildscript release procmacro expandexpr macrocrash serdederive"
grep -E "^(${tests// /|}) (ok|FAIL)" logs/cargo-build-guest$L.log || { tail -30 logs/cargo-build-guest$L.log; exit 1; }
[ "$(grep -cE "^(${tests// /|}) ok" logs/cargo-build-guest$L.log)" = 10 ]

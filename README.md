# 9front-rust

Rust for 9front (Plan 9): the targets `x86_64-unknown-plan9`, `i686-unknown-plan9` and
`aarch64-unknown-plan9` (arm64), with std on 9front's own
C library (as `libp9.a`, its names `__p9_*`), cross-compiled on Linux. C and C++ in crates
are APE programs as any; APE is linked beside the native libc. Programs are linked by
[9front-gcc](../9front-gcc)'s cross gcc (`build-gcc`) against the gcc-built 9front tree,
branch `gcc` (`../9front-gccwt`).

Log of the port (German): `NOTES.md`.

## Layout

- `rust/`, `libc/`, `llvm-project/`: the sources, each its own git repository; the port is
  on their branch `plan9`.
- `tests/`: test programs; `tests/guest.sh` runs programs in the 9front guest.
- `bootstrap.toml.example`: `rust/bootstrap.toml` (`$GCC9`: the 9front-gcc checkout).

## Porting crates

Native Plan 9 calls go through the `libc` crate (its functions are bound to `__p9_*`) or
`#[link_name = "__p9_open"]` & co.: a plain `extern "C" { fn open(...) }` is APE's open.
`scripts/crates-patch.sh diff` warns about such declarations in the crate patches.

## Build

    cd rust && ./x build --stage 1 library --target x86_64-unknown-linux-gnu,x86_64-unknown-plan9
    scripts/build-cargo.sh                     # cargo for 9front
    tests/run.sh; tests/cargo.sh               # in the 9front guest

## Use

    PATH=../9front-gcc/build-gcc/amd64/cross/bin:$PATH
    rust/build/host/stage1/bin/rustc --edition 2024 --target x86_64-unknown-plan9 -O hello.rs

## Install on 9front

`scripts/install.sh` (`objtype=386`, `objtype=arm64`) makes the toolchain for an installed 9front
system, an optional install, as two archives with paths from `/`:

- `out/$objtype/rust-T.tgz`: rustc, cargo, std (T: `x86_64-unknown-plan9`,
  `i686-unknown-plan9`), about 115 MB;
- `out/$objtype/rust-tools-T.tgz`: rustdoc, rustfmt, cargo-fmt, clippy-driver,
  cargo-clippy, about 140 MB.

On 9front, with [9front-gcc](../9front-gcc)'s `/$objtype/gnu` installed (rustc links with
its gcc):

    cd / && tar xzf rust-x86_64-unknown-plan9.tgz && tar xzf rust-tools-x86_64-unknown-plan9.tgz
    bind -b /amd64/gnu/bin /bin; bind -b /amd64/rust/bin /bin
    cargo new hello && cd hello && cargo run

cargo needs the CA roots in `/sys/lib/tls/ca.pem` for crates.io. `tests/tools.sh` unpacks
the archives in the guest and uses rustc, cargo, rustdoc, rustfmt and clippy there. To put
the unpacked tree on the install CD instead: `RUST_OUT=../9front-rust/out/amd64` for
`../9front-gccwt`'s `scripts/guest/mkiso.sh`.


# 9front-rust

Rust for 9front (Plan 9): the target `x86_64-unknown-plan9`, with std on 9front's own
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

## Build

    cd rust && ./x build --stage 1 library --target x86_64-unknown-linux-gnu,x86_64-unknown-plan9
    scripts/build-cargo.sh                     # cargo for 9front
    tests/run.sh; tests/cargo.sh               # in the 9front guest

## Use

    PATH=../9front-gcc/build-gcc/amd64/cross/bin:$PATH
    rust/build/host/stage1/bin/rustc --edition 2024 --target x86_64-unknown-plan9 -O hello.rs

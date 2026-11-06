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

## Window system crates

For Rust programs in windows of wsys or rio (../9front-ui PLAN §6.1): winit,
raw-window-handle, softbuffer and wgpu with backends for 9front, on top of our
own `plan9/p9win` (MIT, only std: the process's window through draw(3), readers
of `/dev/mouse`, `/dev/kbd` and `/dev/wctl`, `/dev/cursor`, label, wctl,
snarf). Their changes are patches like the other crates' (`patches/crates`,
"9front" in them; `cargo --config cargo-plan9.toml`):

- `raw-window-handle-0.6.2`: `RawWindowHandle::Plan9`, `RawDisplayHandle::Plan9`.
- `winit-0.30.13`: `src/platform_impl/plan9` (after orbital): one window a
  process; the readers' input in a queue the loop waits on; a key press waits
  up to 30 ms for kbd(3)'s character (its `text`); size, place, hiding and
  fullscreen through `/dev/wctl`; cursors; `run_on_demand`, `pump_events`;
  `set_visible(false)` neither minimized nor occluded (eframe until its first
  frame).
- `softbuffer-0.4.8`: `src/backends/plan9.rs` (the buffer into the window,
  the damage only); used without its default features.
- `wgpu-30.0.1`, `wgpu-core-30.0.1`, `wgpu-hal-30.0.1`: the Vulkan backend on
  plan9 (ash without dlopen, the static libvulkan.a's `vkGetInstanceProcAddr`,
  `VK_PLAN9_surface`); programs link `-l draw -l 9 -l fmt -l utf -l stdc++`.

9front's `tests/winit` (winittest, eguitest: egui on the processor, eframetest:
eframe on wgpu's Vulkan) uses them; its `scripts/rust/vendor.sh` vendors the
rest. Not yet: glutin (egui's GL), several windows a process, IME.

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


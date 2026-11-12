# 9front-rust — Log

Goal: Rust for 9front, first as a cross compiler (Linux → 9front amd64, core/alloc/std),
std **natively on Plan 9's libc** (`libc.h`, `/amd64/lib.gcc/libc.a`), not on APE.
Later: rustc/cargo natively on 9front, 386.

## Trees (each its own git repo, branch `plan9`)

- `rust/`: rust-lang/rust main (1.101-dev). `bootstrap.toml` (template: `bootstrap.toml.example`,
  `$GCC9` = `../9front-gcc`): LLVM from Rust's CI (`download-ci-llvm`, no LLVM change needed),
  `deny-warnings = false` (std has an unfulfilled lint expectation independent of the port,
  `ineffective_unstable_reexports` in `lib.rs`; check own warnings in the log).
- `libc/`: rust-lang/libc, branch `plan9` of `libc-0.2` (= 0.2.191, the version std uses).
  `library/Cargo.toml` patches `libc` to `../../libc`.
- `llvm-project/`: still unused (for the native phase).

Build: `cd rust && ./x build --stage 1 library --target x86_64-unknown-plan9`
(stage-1 compiler ~4 min, std ~1 min). Program:
`PATH=../9front-gcc/build/amd64/cross/bin:$PATH rust/build/host/stage1/bin/rustc --edition 2024 --target x86_64-unknown-plan9 x.rs`.
In the guest: `tests/guest.sh -f prog... -- 'rc line' ...` (via `../9front`'s `boot-cli.sh`).

## Target x86_64-unknown-plan9 (Part 1)

- `Os::Plan9`, no family (`cfg(unix)` off). LLVM triple `x86_64-unknown-plan9`: unknown
  OS = ELF + System V ABI, like 9front-gcc. Static, `RelocModel::Static`, no PIE,
  `has_thread_local: false`, inline stack probes, `panic=abort` for now.
- Linker = 9front-gcc's `x86_64-pc-plan9-gcc` (linker script `/amd64/plan9.ld` from the driver):
  `-nodefaultlibs` (otherwise APE: `-lap`), instead we pass `-Wl,--wrap=main` ourselves and `-lgcc -lc` late.
  With `-nodefaultlibs`, `crtbegin.o/crtend.o` stay (unwind tables via `.init_array`).
  libgcc's `__wrap_main` (9front-gcc) sets up the FPU like POSIX (9front's kernel otherwise traps on
  1.0/0.0) and calls `.init_array`, then `main`.
- `bootstrap`: target in `STAGE0_MISSING_TARGETS`; `std/build.rs` knows `plan9` (otherwise
  `restricted_std`).

## libc crate (`src/plan9.rs`)

- Types as gcc sees them: `long` = 64 bit, `ulong` = 32 bit (u.h under `__GNUC__`),
  `malloc(ulong)` → allocations ≥ 4 GiB fail. Kernel semaphores are `s32int`
  (port fix in the kernel), `<libc.h>` still says `long*` → Rust `*mut c_int`.
- `await` is a keyword in Rust → `await_` with `link_name`.
- mem*/strlen with `size_t` (rustc checks the runtime symbols against their canonical signature).

## std (`library/std/src/sys/*/plan9.rs`, `pal/plan9/`)

- **Threads = procs** `rfork(RFPROC|RFMEM|RFNOWAIT)`. RFMEM shares everything except the
  stack segment; Rust hands references to stack data to other threads (scoped threads, mpmc
  with packets on the stack) → every thread, `main` too, runs on shared memory.
  `main`: libgcc's `__wrap_main` calls the weak `__plan9_mainstack` (hook from 9front-gcc's
  pthreads), std defines it (`pal::init` references it, otherwise the linker would not pull the object
  from the rlib) and ends with the result via `exits`. Model for the whole mechanism:
  9front-gcc `compat/lib/pthread.c` (rfork in asm, child inherits only R14/R15, runs under
  `PRIVTOP` on its copy of the stack segment, ends there → stack free for `join`).
- **Stacks with a gap** (stack overflow went unnoticed with malloc stacks): each stack its
  own segment (`segattach(SG_CEXEC, "memory", va, 64 MiB)`, pages only on access), with
  1 MiB unmapped in between. A segment that a proc attaches later is not seen by the others
  → `main`'s proc attaches 64 slots before there are any threads (1 GiB below the stack segment, going
  down). Release: `segfree`. More threads or > 64 MiB: malloc stack without gap. Overflow:
  kernel `suicide: sys: trap: fault write addr=…`, proc Broken (notes run on the stack, which
  is gone → no Rust message possible). 9front allows 250 user segments (NSEG = SEG1+250).
- **Beware fork**: slot segments are SG_SHARED, shared even without RFMEM → an `rfork` without
  RFMEM (Command) must take the child off the stack immediately (as with threads).
- **Futex** from semaphores: queue per hash bucket (spinlock), each thread waits on
  its own semaphore (`ThreadData`, libc malloc, shared). Timeout `tsemacquire` (ms,
  rounded up), a note interrupts → spurious wakeup. On top of that std's futex Mutex/Condvar/RwLock/
  Once/Parker.
- **TLS**: one `privalloc` slot (lives on the private stack segment) → `ThreadData` → table
  with 1024 keys. The child inherits the copy of the slot → zero it first. Destructors at thread end
  by itself (guard "retained" like Xous/Hermit).
- **Errors** are strings: `io::Error::last_os_error()` → `Error::new(kind, errstr)`,
  `kind` from the kernel/file server messages (`port/error.h`). `raw_os_error()` = None.
- **Exit**: `exit(0)` → `exits(nil)`, otherwise `exits("<code>")`. Other threads via
  `/proc/n/ctl kill`; if a thread ends the program, `main`'s proc gets the status as a note
  (unhandled → ends with the text as status), so that the parent process sees it.
  `abort` = status `abort`. Note "sys: write on closed pipe" is ignored (like SIGPIPE).
- Time: `Instant` = `uptime()` (ns since boot), `SystemTime` = `nsec()`. Randomness `/dev/random`.
  Parallelism: lines of `/dev/sysstat`. `sleep` in ms, rounded up.
- malloc returns 8-byte alignment (Bhdr 8 + Tag 8?) → larger via `mallocalign`.

## std Part 2: fs, env, processes, network, unwinding (2026-10-29)

- **fs**: `open`/`create` (create on an existing file truncates → `create` without truncate = open, otherwise
  `create|OEXCL`, race → open again), stat(5) `Dir` as FileAttr (uid/gid/muid, qid via
  `os::plan9::fs::MetadataExt`), `dirread` piecewise. No append mode in open → seek to the end
  before every write. rename = wstat of the name only within the same directory (comparing the qids of
  the parents), replaces nothing → remove the target first; across directories: copy the file (permissions,
  times) + remove, directories: CrossesDevices. No links, locks, fsync (Ok).
- **`fd_view`**: std assumes `File`, `TcpStream`, … are exactly one `RawFd` in size → extra state
  (append, timeouts, UDP peer) in a table per descriptor (`sys::fd::FdState`), deleted in
  `OwnedFd::drop`, set by everyone who makes a descriptor.
- **close-on-exec**: pipe(2)/dial give none; `open("#d/N", mode|OCEXEC)` returns a copy with
  OCEXEC (devdup returns the channel itself, fd2path stays the old name). `dup(a, b)` clears
  OCEXEC on b (`f->flag[fd] = 0`).
- **env**: `/env` files; an inner `\0` (rc lists) stays, `split_paths`/`join_paths` split on `\0`
  (`$path`). `current_exe`: argv[0], without `/` via `$path` (Plan 9 does not know the program name).
- **Processes**: `rfork(RFPROC|RFFDG|RFREND[|RFENVG|RFCENVG])` via asm (`rfork_run`), child immediately on
  its stack segment (the slots are shared even without RFMEM!), no allocation/locks in the child
  (memory copy, malloc lock possibly held). Exec errors via OCEXEC pipe. Program lookup in
  `$path` like rc. Waiting: every thread reads `/proc/<spawner>/wait` (one per spawner, results
  for others in a table) → `wait` from any thread. Limit: child of a finished thread is
  orphaned (Plan 9 does not pass children on). `try_wait`: Plan 9 has no zombies → `/proc/pid` gone
  = finished. ExitStatus = Waitmsg text (Copy → `[u8; ERRMAX]`), `code()` = number after ": ".
- **Network**: dial/announce/listen/accept. fd2path of an announce ctl is `/net/tcp/clone` → reopen the ctl
  by name. UDP with `headers` (52 bytes). Timeouts via `alarm` (note "alarm" is swallowed in the
  handler). `shutdown(Write)` = ctl `close`. Names via `/net/cs`. The test guest lacks
  loopback: `ip/ipconfig loopback /dev/null 127.1`.
- **Unwinding**: `panic=unwind` default; libgcc's unwinder (unwind-dw2-fde in libgcc.a, crtbegin.o
  registers `.eh_frame`), `unwind`/`panic_unwind`/personality like unix. Backtraces: submodule
  `library/backtrace` (branch `plan9`): libunwind trace, gimli reads `/proc/<pid>/text`.

## cargo for 9front (Part 3, 2026-10-29)

`scripts/build-cargo.sh` builds cargo (`rust/src/tools/cargo`, branch `plan9`) for
x86_64-unknown-plan9; `tests/cargo.sh` tests in the guest: `--version`, `new` (libgit2), `search`
(crates.io API: curl + rustls), `fetch` (index, downloads, unpacking, sqlite), all ok.
Without rustc on 9front, an rc script gives the answers of the stage-1 rustc (`-vV`, `--print`).

- **C libraries via APE: ape9** (`scripts/ape9.sh`, `ape9/`): libap, libbsd, 9front-gcc's
  libapecompat (without `pthread.o`, which defines `__plan9_mainstack` like std) with **every
  defined symbol renamed** `__ape_<name>` (objcopy `--redefine-syms`, 1000+ symbols);
  the crates' C libraries likewise: `scripts/ape9-ar` is the cc crate's `ar` and
  renames the archive afterwards. So APE's `open`, `read`, `malloc`… do not collide with libc's. APE's
  `malloc.o` is dropped (malloc/free/realloc = libc's, one heap; aligned_alloc &c. in
  `ape9init.c` via `mallocalign`). `ape9init.c` (from `.init_array`) does what APE's `_main`
  would do: `_tos`, errno per proc (one `privalloc` slot), `_envsetup` (but without `_NOTIFY`: std has
  the note handler; via objcopy to `__ape9_nonotify`), flush APE's stdio at exit (libc's
  `atexit`: Rust ends via `exits`). Link: `-u __ape9_init`, group of the libape9-*, then `-lc`
  again (ape9 sits behind `-lc`).
- **APE's `select` is unusable** in a Rust program: it forks copy procs (without
  RFMEM) on the running stack, which is shared here (stack slots are SG_SHARED), attaches
  its buffer segment only on the first call (other procs do not see it → fault),
  only one waiter. Replaced by `ape9/ext/buf.c` (`_startbuf`, `_readbuf`, `_closebuf`,
  `select`, `poll`): copy procs RFMEM|RFFDG via std (`__plan9_rforkproc`: own
  stack segment, registered as a thread → ends with the program), buffer from malloc,
  any number of waiters on their own semaphores. The copy proc closes all foreign
  fds (RFFDG copies the shared fd table of all threads → kept e.g. the server socket
  of another thread open, no EOF), with raw calls (`_fdinfo` is shared).
- ape9/ext/posix.c: fsync, getsid/getpgid, utimes, getpwuid_r/getpwnam_r, fchown (no-op),
  posix_fallocate (ftruncate), **getsockopt** (APE's returns -1 without errno → curl read a stale
  `errno` = EAGAIN as a connection error; SO_ERROR=0). ape9/ext/pthread.c: mutexes only
  (Drepper + semaphore, recursive per proc) for sqlite. ape9/include: supplementary headers
  (`#include_next`): poll.h, unistd.h, sys/socket.h (HAVE_SOCK_OPTS, fd_set), netinet/in.h …
- libc crate: APE's POSIX types (off_t, fd_set, sockaddr, …) and errno values (APE: EINVAL=12,
  ENOMEM=23!) for the bindings of the C libraries.
- **Crates** (`patches/crates/*.patch`, `scripts/crates-patch.sh apply` → `crates/`,
  `cargo-plan9.toml` = `[patch.crates-io]`; `cargo update -p libc` after patch changes):
  getrandom 0.2/0.3/0.4 (`/dev/random`), ring (Linux ABI list: SysV + ELF → asm), rustls-
  native-certs (`/sys/lib/tls/ca.pem`), rustls-platform-verifier, curl (socket via
  `__ape_socket`, no socket2; curl-sys with **rustls** instead of OpenSSL, ring instead of aws-lc),
  curl-sys (no threaded resolver, no Unix sockets/socketpair, `CURL_CA_BUNDLE`),
  libgit2-sys (without TLS/SSH — git via curl through git2-curl —, without threads, poll, NO_MMAP),
  libssh2-sys empty, libsqlite3-sys (without mmap/WAL/extensions/worker; locks with
  "unix-dotfile" via SQLITE_EXTRA_INIT, APE's fcntl has no record locks), git2, tar
  (like wasm32, but set permissions), url (file paths), filetime, home, is_executable,
  gix-sec (`/dev/user`), gix-config-value (`/usr/<name>`). Environment: `LIBZ_SYS_STATIC=1`
  (APE's zlib.h is old).
- cargo itself: cargo-util (exec_replace = spawn+wait, read2 with two threads, paths),
  cargo-credential (`/dev/cons`), terminal without escapes, job (note group), flock (Plan 9 has
  no locks → like a file system without), FileId, `$path` instead of `PATH`.
- std fix: `Path::is_absolute` (Plan 9 was not in the unix-like list).
- **CA roots**: 9front checks TLS with thumbprints, has no CA bundle: cargo expects
  `/sys/lib/tls/ca.pem` (or `http.cainfo`). For the tests, the host's.
- After every std rebuild: `rm -rf tests/out/cargo-port` (otherwise "possibly newer version of
  crate std"), and always build std for both targets (otherwise one is missing from the sysroot).

## APE alongside the native libc, without ape9 (Part 4, 2026-10-29)

The user wanted C from crates to link without ape9 renaming. Solved the other way round: the
**native libc** is renamed, APE stays as it is.
- 9front (branch `gcc`): 9front-gcc's compat is in APE (`sys/src/ape/lib/ap/gnu`,
  `sys/src/ape/lib/musl`, headers under `__GNUC__`); `libp9.a` (`scripts/build/libp9.sh`, post
  step of libc): libc.a with all names as `__p9_*`; `ap/gnu/native.c`: `__ape_attach`
  (constructor 101: `_tos`, errno/`_privates` in privalloc slots, `_envsetup` without notes),
  `__ape_exit`; empty `libm/libpthread/librt/libdl.a`. 9front-gcc now links only `-lbsd -lap`
  (no libapecompat), build in `../9front-gcc/build-gcc` against `../9front-gccwt` (`GCC9BUILD`),
  until `gpu` merges the branch `gcc`.
- libc crate: every native function `#[link_name = "__p9_…"]`. Target: entry `__p9__main`,
  `--start-group -lp9 -lbsd -lap -lgcc --end-group`. std: `malloc`, `free`, `realloc`,
  `aligned_alloc`, `memalign`, `posix_memalign` (16 bytes, on libc's mallocalign; APE's malloc.o
  does not get into the link: one heap), `__ape_exit` in `end_program`.
- C/C++ of the crates: normal APE translation with `x86_64-pc-plan9-gcc`/`ar`; APE's pthreads
  run on std threads (`__plan9_thread_spawn`), select/poll via APE's new buffering.
- Found: APE's `uintptr_t` was `unsigned long long`, `size_t` `unsigned long` (curl ↔
  rustls-ffi; ape9's `-Wno-error=incompatible-pointer-types` had hidden it): for gcc now
  the compiler's types. After a rustc rebuild, build std for **both** targets in one run
  (`--target x86_64-unknown-linux-gnu,x86_64-unknown-plan9`), otherwise one is missing.
- **Rule for crate ports and own code:** native Plan 9 calls only via the libc crate
  or with `#[link_name = "__p9_…"]`. An own `extern "C" { fn open(…) }` silently ends up at
  APE's `open` (different flags: OCEXEC 32 means something else there); that is how it was in the getrandom ports,
  the tests ran anyway. `scripts/crates-patch.sh diff` reports such declarations.
- Tests: `tests/run.sh` 7/7 (new `cmain`, `cxxmain`: C and C++ from `tests/c`), `tests/cargo.sh`
  5/5 (version, new, search, fetch, refetch).

## rustc on 9front (Part 5, 2026-10-30)

`./x build --stage 2 --host x86_64-unknown-plan9 --target x86_64-unknown-plan9 compiler/rustc`
builds rustc for 9front: static, LLVM (build/llvm-plan9) built in, 575 MB with debug info.
- LLVM against the new APE: `SIGTRAP` was missing in APE; CMake's FindThreads guessed `-lpthreads`
  (its probes cannot link): told libap about threads. bootstrap had reset the submodule
  llvm-project to Rust's commit (`submodules = false`, gitlink to plan9).
- bootstrap: a target with its own `llvm-config` that runs on the build host
  (`scripts/llvm-config-plan9.sh`) gives `rustc_llvm` this one instead of the host's.
- Compiler: `rustc-main` takes `rustc_driver_impl` directly (no dylib); `dll_path` is the
  program itself (`current_exe`); memmap reads the file (like wasm); no ctrlc;
  `path_to_c_string` via `os::plan9`; libc, libloading (no dylibs: opening fails),
  getrandom 0.3.3 from `crates/` via `[patch.crates-io]`.
- **No proc macros** in a rustc on 9front (dylibs). Plan: proc-macro crates as small
  programs, the bridge (`Buffer` messages as with `cross-thread`) over a pipe.
- Linker on 9front: the native gcc of 9front-gcc (`out-gcc`, `x86_64-pc-plan9-gcc`), plus
  `/amd64/include`, `/amd64/lib.gcc` (with `libp9.a`), `/amd64/plan9.ld`, `/sys/include` of the
  tree with the new APE. The native gcc lacked `getpagesize` (libbacktrace now uses
  mmap), in its own object file (libiberty has its own).
- `tests/rustc.sh`: rustc --version, compiles `tests/std1.rs` in the guest and runs it:
  all ok (MEM=3072). `tests/guest.sh -d /guest/dir=hostdir` copies directories.

## cargo build on 9front (Part 6, 2026-10-30)

`tests/cargo-build.sh` (rustc, cargo, native gcc, headers/libraries of the gcc tree in the guest;
`CMDWAIT` high: boot-cli otherwise waits 60 s per line): `cargo new` + `cargo build` + run ok,
`itoa` from crates.io ok. Found: jobserver 0.1.34 fell back to its wasm stub (`configure`
→ panic) → jobserver only in-process; 9front's kernel checks the buffer address even for 0 bytes
(empty slice: 0x1) → std `buf_ptr`; incremental rustc needs locks → plan9: without locking
(like cargo); rustc finds its sysroot via its own path: as `/bin/rustc` (bind) that would
be `/` → on plan9 fall back to `/$cputype/rust` (rustc_session::filesearch).
- serde (build script), release build: green (`tests/cargo-build.sh` 6/6, 4 CPUs, ext4 image,
  ~2 min). Found along the way: libstdc++ without TLS → `call_once` deadlocks with several CPUs
  (LLVM's pass initialization; 9front-gcc: HAVE_TLS via emutls); ext4srv `truncate` to the same
  length and `qid.vers` unchanged on writes (the kernel started freshly built
  programs with old text: invalid opcode) — both fixed in the 9front branch gcc; guests with
  4 CPUs (`SMP`), trees on an ext4 image (`guest.sh -D`). Earlier: in the second diagnostic run rustc was `Ready` (computing), cargo's `PREAD`
  procs are APE's copy procs of the curl connections. Rather very slow than a deadlock: measure
  (time for serde in the guest, `-Z time-passes`), check the allocator (everything via mallocalign 16?).
  Log: `logs/cargo-dbg-serde-hang.log`.
- **File locks with DMEXCL** (2026-10-30): `std::os::plan9::fs::lock_file(path, wait)` returns
  a `FileLock`: the file opened alone as an exclusive-use (DMEXCL) file; a second
  open fails ("file is locked" gefs/cwfs/ramfs, "file locked" hjfs → `WouldBlock`),
  `wait` retries every 100 ms. New lock files are created with DMEXCL (create), old ones get
  it via dirfwstat and are reopened. No shared locks (shared = exclusive).
  - open(5): the server may break the lock of an unused file (cwfs `TLOCK` 5 min,
    hjfs `EXCLDUR` 300 s, after that even reading/writing by the holder fails there; gefs counts
    only references). Like upas' `trylock`, a thread of the lock reads one byte every minute;
    drop ends it (Condvar) and then closes the file (hint from a side agent).
  - 9front's ext4srv does not know DMEXCL (the bit does not stick): after a wstat attempt
    `lock_file` returns the file without a lock (like upas: "live without the lock"), no hang.
    In the gcc branch (9front-gccwt) ext4srv now holds DMEXCL (inode flag UNRM, one open at a
    time, "file is locked"): the cargo guests (`PLAN9=../9front-gccwt`) really lock.
  - std's `File::lock*` stay `Unsupported` (a lock here needs the open itself).
  - cargo (`util/flock.rs`, `mod plan9`): only cargo's own lock files (`.cargo-lock`,
    `.cargo-build-lock`, `.cargo-artifact-lock`, `.package-cache`, `.package-cache-mutate`,
    `.lock` of the build units) get DMEXCL; data files (`Cargo.lock`,
    `credentials.toml`, `.crates.toml`, packages) do not: DMEXCL would stay on them, and cargo opens
    `Cargo.lock` twice in the same process. A process shares its locks of a file (one
    table `HELD`), otherwise it would lock itself out; waiting outside the table, "Blocking" as
    usual. Not writable → without lock.
  - rustc (`flock/plan9.rs`): close the previously opened file, then `lock_file(p, false)`.
  - Test: `tests/lock.rs` in `tests/run.sh` (ramfs: exclusive, wait until drop, old file becomes
    DMEXCL, drop immediately; `/tmp`: as the server has it); by hand on ext4srv (gcc branch): exclusive like ramfs, e2fsck ok.
- `abort` from C: `__plan9_abort` in std ends the other procs, the aborting one ends with
  `ud2` and stays Broken (APE `gen/abort.c`). Tested (`cexit-abort`). acid does not read the
  ELF symbols of the gcc port ("no symbol information"): PC from `/proc/N/status`/acid,
  on the host `addr2line -e binary PC`.

## Proc macros as programs (Part 7, 2026-10-30)

Plan 9 has no dylibs; a proc-macro crate for a plan9 host becomes a static
program, rustc talks to it over stdin/stdout (`library/proc_macro/src/bridge/remote.rs`).
- The bridge exchanges only `Buffer`s anyway (handles as numbers, symbols as strings), like the
  cross-thread strategy; `RemoteStrategy` sends them through a pipe: `R` (index, input),
  the program asks back with `D` (rustc: `Dispatcher::dispatch`, reply `r`), result `O`.
  Names and kinds of the macros are in the metadata; the program reports only the count (`L`).
  Nested runs (expand_expr) get a second instance. stdout of the macro → stderr.
- Compiler: harness generates `#[rustc_main] fn main() { serve(_DECLS) }`; `entry_fn` for plan9
  proc-macro; link as `StaticNoPicExe`, no export list, `--undefined` of the metadata symbol
  (otherwise `--gc-sections` throws `.rustc` away); `invalid_output_for_target` allows it;
  `rustc_metadata` loads with `remote::load`. `Client` has a field `remote` on plan9.
- When cross-building rustc, bootstrap builds proc macros for the target too (dual proc macros): these
  are now these programs (only their metadata is needed).
- `tests/cargo-build.sh` 9/9: own derive/attribute/bang macro, nested macros
  (`expandexpr`: `TokenStream::expand_expr` on a macro of the same crate, three levels deep,
  so four instances of the program at once; a built-in macro `concat!` above it),
  serde-derive + serde_json (~5 min, 4 CPUs). On Linux the same macros: same output.
  `macrocrash` (10/10): an inner macro that panics (as on Linux: an error "proc macro
  panicked", "message: boom inside", the instance keeps running) and one that ends with `exit`
  (error "proc-macro program failed"; the dead instance does not go back into the pool, the
  next run gets a new one); the macros after that expand correctly (`assert!`s hold).

## 386: i686-unknown-plan9 (Part 8, 2026-10-30)

Cross (Linux → 9front 386), decision: SSE2 like Rust's i686 targets (`cpu = pentium4`); the
tree itself is `-march=pentium` (x87), the C ABI is the same. 9pc loads MXCSR with 0x1f80
(SSE exceptions masked), libgcc's `__wrap_main` sets the x87 control word as on amd64.
- Toolchain: `objtype=386 BUILD=$PWD/build-gcc OUTDIR=$PWD/out-gcc PLAN9=../9front-gccwt
  ./build.sh cross` in 9front-gcc (against the 386 tree, `CC=gcc ARCH=386 build.exe -D` in
  9front-gccwt; after that the next amd64 run there rebuilds everything: shared objects).
- Target `rustc_target/.../i686_unknown_plan9.rs`: data layout of i686-unknown-linux-gnu
  (System V i386: double and long long in structs 4-byte aligned, as gcc's), otherwise
  `base::plan9`. `bootstrap.toml`: `[target.i686-unknown-plan9]` (template), not in
  `build.target`: `./x build --stage 1 library --target i686-unknown-plan9` explicitly.
  After every compiler rebuild, build std for all targets (otherwise one is missing from the sysroot,
  E0463): `--target x86_64-unknown-linux-gnu,x86_64-unknown-plan9,i686-unknown-plan9`.
- std: the assembler in `pal/plan9/proc.rs` for x86 (`runon`, `rforkthread`, `rforkrun`):
  system call `INT $64`, number in AX, arguments from 4(SP); on 386 the child gets all
  registers of the parent (forkret: POPAL of the copied Ureg), AX = 0, so ESI/EDI
  instead of R14/R15. The stack slots stay (only SEG1–SEG4 free per proc: ≤ 4 slots of 64 MiB,
  from 0x80000000 downwards). libc crate: `usize_` = `c_ulong`, `jmp_buf` 6 words on 32 bit.
- `objtype=386 tests/run.sh`: all 12 ok in the 9pc guest (qemu-system-i386, 4 CPUs), first try;
  outputs in `tests/out/$objtype`, log `logs/guest-tests.386.log`.
- Stack: 9front programs on 386 start with only 4-byte alignment (kernel, notes; the
  tree `-mincoming-stack-boundary=2`). LLVM assumes 4 for i686 with unknown OS anyway
  (16 only for Linux, Darwin, kFreeBSD, 64 bit) and aligns frames with 16-byte data itself
  (`andl $-16, %esp`, checked); the `S128` in the data layout is only a hint (one
  different from LLVM's would be an error for rustc).
- **cargo for 386** (`objtype=386 scripts/build-cargo.sh`, `objtype=386 tests/cargo.sh`: 5/5
  in the 9pc guest: version, new/libgit2, search/curl+rustls, fetch/sqlite, refetch). Two
  C errors, both `-Wincompatible-pointer-types` (an error since gcc 14) from one cause: gcc
  for i686-pc-plan9 had `size_t` = `unsigned long` (gcc's default, like Plan 9's `usize`),
  but `uintptr_t` = `unsigned int` (glibc stdint.h for 32 bit). curl's `rustls.c` takes
  `uintptr_t *` for rustls-ffi's `size_t *` (as on amd64 before APE's stdint fix). Fixed at
  the root: 9front-gcc `i386/plan9-386.h` INTPTR/UINTPTR_TYPE `long` (like size_t and u.h's
  `uintptr`; same size, no ABI change), APE's `inttypes.h` PRI*PTR `l` for it (only
  under `__plan9__`: the tree itself builds with gcc -m32, where it stays int). libgit2 took
  `__builtin_uadd_overflow` (`unsigned int *`) when `SIZE_MAX == UINT_MAX`: now
  the type-generic `__builtin_add/mul_overflow` (crate patch).
- **rustc native on 386**: native gcc for 386 (`objtype=386 BUILD=… OUTDIR=$PWD/out-gcc
  PLAN9=../9front-gccwt ./build.sh hostlibs native plan9` in 9front-gcc, the linker in the guest);
  LLVM `objtype=386 scripts/build-llvm.sh` (`build/llvm-plan9.386`, `cmake/plan9.cmake` by
  `$objtype`), `objtype=386 scripts/llvm-config-plan9.sh`, `llvm-config` in
  `[target.i686-unknown-plan9]`; then `./x build --stage 2 --host i686-unknown-plan9 --target
  i686-unknown-plan9 compiler/rustc` and `library` likewise: static, 32 bit, 522 MB with
  debug info. Without a change to the compiler. `objtype=386 tests/rustc.sh` 3/3,
  `objtype=386 tests/cargo-build.sh` 10/10 (all stages as on amd64, also proc macros as
  programs, nested and crashing; serde-derive 2 min 18 s, 4 CPUs, 3 GB).
- After rebuilding the stage-1 compiler (new target), the stage-2 std for amd64 was missing: rebuild the
  whole amd64 chain (stage 2 rustc for plan9, library).
- `./x build --stage 2 library --host T --target T` removes the stage-2 std (and the stage-1
  sysroot) of the other plan9 targets: before `tests/rustc.sh`/`cargo-build.sh` of an architecture
  build its stage-2 library, afterwards `./x build --stage 1 library --target …` for all (otherwise
  E0463 in `run.sh` and cargo).

## std test suite in the guest (Part 9, 2026-10-30)

`./x test library/std --target x86_64-unknown-plan9 --no-doc --no-fail-fast` with
`runner = tests/std-runner.sh` in the `[target.*-plan9]` of `bootstrap.toml`: cargo calls it with
each test program; it boots a guest of its architecture per program (`file`: 386 or
amd64), `/tmp` on ext4, loopback `127.1` and `::1`, 900 s limit, and returns output and status
(`STDTEST_LOG`: all guest logs).
- Two copies of std in the unit test program (the one under test and the sysroot's via libtest):
  the assembler is now naked functions (mangled, each copy its own; `threadstart` via
  `sym`), the C exports (`malloc` &c., `__plan9_*`) have `no_mangle`/`export_name` only except
  `cfg(test)`, `PRIVTOP`/`MAIN_PID` one instance (sgx' pattern: `export_name` +
  `linkage = "available_externally"` in the test). `current()` takes ThreadData only with the
  own pid: the private slots of a proc are a copy of the parent proc's (rfork).
- std corrections: `Instant` signed (nanoseconds, i64 seconds range like
  timespec: `Instant - 10 years` did not work); `OpenOptions` checks like unix (messages,
  append+truncate); `set_times` sets the mtime, atime silently skipped (stat(5): a wstat does not
  change it; variant "compatible", described in `os::plan9::fs`); `remove_dir_all`
  like `rm -r` (read completely, then delete: otherwise entries shift, 32768 files);
  `ExitStatus::code` like APE's wait (number, otherwise 1, note → none); NUL in the program name
  `InvalidInput`; "malformed address" `InvalidInput`; `read_buf` of sockets without prior
  initialization; `Socket::as_raw`.
- ext4srv (gcc branch): atime on creation; wstat length extends with zeros, truncates with
  zeroed block remainder; `rwrite` reads the length fresh from the inode (another fid changed
  it: the padding otherwise wrote over its data).
- Tests on plan9 `ignore`d with reason: symlinks/junctions (14 + 5), hard links, directory
  rename across directories, setting atime (3), TCP: half close (`shutdown_smoke`,
  `close_read_wakes_up`), reading back keepalive/ttl/hop_limit, linger, nonblocking, peek,
  Nagle; NUL in environment values (rc lists). On the test side: path separator `/`, rwlock limit like
  futex, `$path` instead of `PATH`, `env` = `rc -c 'cd /env; for(v in *) whatis $v'`, shell
  `/bin/ape/sh`.
- **Results** (2026-10-31): std amd64: unit 343 ok, 39 ignored, 2 failed
  (`multiple_connect_interleaved_*`, TCP under load), all 29 integration programs ok;
  std 386: unit 345 ok, 38 ignored, 0 failed (the two ext4srv tests only with
  the rebuilt 386 ext4srv), 29 ok. `./x test library/coretests library/alloctests`:
  amd64 and 386 without failures, without adaptation (core 2970 and 2969 respectively, 2 ignored as upstream;
  alloc 339/1494/643 and 338/1495/643 respectively, the two alloc_error programs).
- **Test hang under load: libgcc's unwinder without locks.** pstack showed a Broken
  proc in `__plan9_abort` ← `_Unwind_RaiseException_Phase2` (panic in a test thread):
  gthr-posix checks via a weak reference to `pthread_cancel` whether threads exist; that
  does not pull APE's pthread.o into the Rust program, the FDE lists stayed unlocked. Fixed in
  9front-gcc (`GTHREAD_USE_WEAK 0` in `gcc/config/plan9.h`). The `abort` of the test copy of std
  ended only its threads: main (sysroot std) waited forever.
- **Solved 2026-11-11: TCP under load** (formerly two open items: with 12 test threads
  `multiple_connect_interleaved_*` failed with "connection refused", "i/o on hungup
  channel", once "listen hungup"; and `connect` over loopback hung, reproduced with C in
  `tools/csstress.c`, `tools/csstress.rc`, also with the kencc kernel). Cause in 9front, in
  `devip.c:closeconv`: it set `inuse` to 0 before `tcpclose` ran, and that releases the
  conv lock in `tcpoutput`. If the ACK for the FIN arrived meanwhile, `Fsprotoclone` reassigned the
  connection, and the old `closeconv` afterwards set the state of the new owner to
  Idle: its `connect` slept forever. Fix in ../9front-gccwt (`gcc` 237bfdd4b, patch
  `patch-upstream/0025`, details there in MEMORY.md). After that with 12 test threads the
  network tests green in 6 of 6 runs (61 ok, ~1.1 s each; `logs/stdtest.net-fix*.log`), csstress with
  12, 24, 50 procs passes. std still chooses numeric addresses directly (`/net/tcp/clone`), cs
  is unnecessary for SocketAddr.
- **Open: TCP half close.** 9front's ctl `close` also hangs up the read side (`tcpclose`:
  `qhangup(c->rq)`), so `shutdown(Write)` ends both directions: a reply from the
  peer is silently lost (HTTP clients, nc-style tools). Solvable only in the kernel.

## Tests

- `tests/run.sh`: builds `tests/*.rs` and `tests/c`, runs in the guest, one line per test; state: all 7 ok.
- `tests/nostd.rs` (core, libc by hand), `tests/std1.rs` (args, scoped threads, Mutex under
  load, Condvar/timeout, Barrier, RwLock, channels rendezvous/buffered, TLS + destructor,
  sleep, SystemTime, HashMap, float format), `tests/overflow.rs` (overflow in main and thread
  → Broken): run in the guest (2026-10-29).

## async: mio and tokio (Part 10, 2026-10-31)

Plan 9 has no epoll/kqueue/poll for native fds, only blocking calls.
- **std** (`sys/net/connection/plan9/pump.rs`): a socket that is switched to nonblocking or
  peeked gets a pump: a reader proc into a buffer (stream 64 KiB, datagrams,
  accepted calls of a listener), a writer proc from a buffer (only with the first
  write: a read-only socket costs one proc). read/peek/recv/accept from that or
  WouldBlock, write into it; own fd (dup), when the last fd closes the reader is interrupted by
  note (`rust: interrupt`), the writer writes out. `shutdown(Write)` only after
  the buffered data, via the ctl opened at the call (a later open by name found the
  number already assigned to another connection). `os::plan9::net`: `readiness`,
  `set_ready_hook`, `connect_nonblocking` (ctl created, data opened right away, connect in the proc).
  Peek now works blocking too. `tests/nb.rs` (in run.sh).
- **mio** (`crates/mio-1.2.1`, `sys/plan9`): selector = list of changed fds + Condvar, the
  hooks register; `select` reads `readiness`. Waker, connect/bind via std (no socket before
  the connect). `tests/crates/mioecho`.
- **tokio** (`crates/tokio-1.54.1`): without socket2 on plan9 (no BSD sockets): no
  `TcpSocket`, linger/UDP options Unsupported, `peek_sender` via std's peek_from.
  `tests/crates/tokioecho`: multi-thread runtime, 20 echo clients with half close, sleep,
  timeout, channel, fs. `tests/async.sh` (both architectures).
- **Kernel** (9front-gccwt): TCP half close (ctl `shutdown`), a stale bypass pointer
  on loopback set up new connections (MEMORY.md there).
- Cost: one to two procs per nonblocking connection (9front's limit: conf.nproc).

## HTTP, HTTPS, DNS (Part 11, 2026-10-31)

- **hyper-util** (`crates/hyper-util-0.1.21`): the `HttpConnector` (client-legacy, used by
  reqwest) builds its socket with socket2 and tokio's `TcpSocket`; on plan9 it connects via
  `tokio::net::TcpStream::connect` (std's connect in the background). Local address: Unsupported;
  keepalive is not set (TCP's ctl `keepalive` is not in std).
- `tests/crates/httptest`: hyper server (HTTP/1.1 and HTTP/2, auto), hyper clients (http1,
  http2 with 10 streams), hyper-util's client (pool), reqwest (GET, POST 1 MiB, JSON, chunked,
  404, timeout, 20 simultaneous, HTTP/2).
- `tests/crates/tlsdns`: DNS via `/net/cs` (std and tokio `lookup_host`, localhost, an
  unknown name, 10 simultaneous), HTTPS with rustls (ring) against a local
  tokio-rustls server with rcgen certificate (HTTP/1.1, HTTP/2 via ALPN, 1 MiB, unknown
  issuer rejected) and to crates.io with the roots from `/sys/lib/tls/ca.pem`. Needs the
  network (QEMU's DHCP/DNS).
- 9front bug found: ndb/cs truncated error messages to 63 characters (`curerr[64]` in
  `rwrite`), with long names "name does not exist" was missing; fixed in the gcc branch.
- `tests/async.sh` runs all four (mio, tokio, http, tlsdns).
- Sporadic "early eof" in tokioecho (20 clients at once): with a full call queue
  (`Maxincall` 10), 9front's kernel set up a call with reset that the client already considered
  connected (its connect had finished): it read EOF immediately ("eof after 0", with
  `Fsnewcall: incall queue full`). Fixed in the gcc branch (9front-gccwt MEMORY.md): the call
  stays in limbo as on BSD/Linux with a full backlog. tokioecho checks it specifically: a
  std listener accepts nothing for one second, 30 clients connect (old kernel: 20
  read EOF).

## tokio::process and tokio::signal (Part 12, 2026-10-31)

- **std** `os::plan9::note`: `catch(name)` catches a note (exactly this name; not `sys: …`,
  `alarm`, `rust: …`): the program no longer dies from it, `Note::wait`/`take` count it
  (semaphore, `semrelease` in the note handler: no lock, no allocation). A note to the
  group (Delete, `notepg`) reaches every proc of the program; it is counted only in the proc
  of `main` (`proc::main_pid`), the others carry on. The kernel leaves out the sender
  of a `notepg` (`postnotepg`: `p == up`): whoever posts from main to its own group
  does not reach main. Plus fd traits for `ChildStdin/Stdout/Stderr` (like unix).
- **Limitation**: a caught note interrupts blocked system calls in all procs
  ("interrupted", `ErrorKind::Interrupted`). Unix mostly restarts them (SA_RESTART), Plan 9
  does not: std's own places (pump, parking, wait, read_exact/read_to_end) and tokio's
  child pipes retry, a single `read` of a program may see the error.
- **tokio::signal** (`signal/plan9.rs`): `ctrl_c()` = note `interrupt`,
  `tokio::signal::plan9::note(name)`; one thread per note waits on the semaphore and wakes a
  watch channel (like tokio's registry under unix; notes before looking coalesce).
- **tokio::process** (`process/plan9.rs`, after windows.rs): a waiter thread per child forks
  it itself and waits (std's `wait`): the Waitmsg goes to the forking proc, which thus lives
  until the child ends (a dropped child is also reaped). Pipes blocking in the
  blocking pool (`Blocking`, Plan 9 pipes are not pollable), retry on
  "interrupted". kill via `/proc/<pid>/ctl`. Cost: one proc per running child.
- `tests/crates/tokioproc` in `tests/async.sh` (in its own note group: `rfork s`, exec):
  output, exit status, 1 MiB through cat, kill, kill_on_drop, try_wait, 10 simultaneous, not
  found, std counting (once for the group), ctrl_c, a pipe across the note,
  plan9::note.
- **Listener closes synchronously** (found: `nb` "refused" on 386, red since Part 10): the
  reader of a pump blocks in `listen` and holds the announcement; `drop` only gave the note
  and returned, a connect following right away was still accepted and then closed
  (EOF instead of "refused"). Now closing waits for the reader to end; the note is sent again every
  10 ms (if it arrives before it is in `listen`, it fizzles), at most 5 s. The port
  is free when close returns, as under unix.

## Installation and tools (Part 13, 2026-10-31)

- **Rebase** of the gcc branch onto front 350266104 (27 commits from upstream, no conflicts;
  manifests regenerated, `news.c` gone): all three architectures build, run.sh and async.sh on amd64
  and 386 green.
- **rustdoc, rustfmt, clippy for 9front** (`./x build --stage 2 --host T --target T
  src/tools/rustdoc src/tools/rustfmt src/tools/clippy`): static like rustc, without the
  `rustc_driver` dylib: `extern crate rustc_driver_impl as rustc_driver` (rustdoc, rustfmt's
  bin, clippy-driver), where it is only needed for linking it is dropped (rustfmt's lib,
  clippy_utils); cargo-clippy gets `EXIT_FAILURE` from a small module instead of the whole
  compiler. bootstrap puts the rlibs of the compiler crates into the sysroot for plan9 (otherwise only
  rmeta: on Linux they are in librustc_driver.so). rustdoc: `Os::Plan9` in `clean/cfg.rs`,
  its own `flock` takes `os::plan9::fs::lock_file` (DMEXCL). Crates: stacker 0.1.21 (a
  stack from malloc like wasm/hermit: std finds its thread data via `_privates`, not via
  the stack pointer), dirs-sys 0.5.0 (`$home`, no XDG directories). The tools are in
  `build/x86_64-unknown-linux-gnu/stage2-tools/T/release` (stage2/bin is recreated by bootstrap every
  time). Stripped: rustdoc 165 MB, clippy-driver 169 MB, rustfmt 49 MB (amd64).
- **Installation** (`scripts/install.sh`): `out/$objtype/$objtype/rust` (bin, lib/rustlib/T/lib
  with rlib and rmeta: bootstrap builds std with `-Zembed-metadata=no`, without the rmeta
  rustc finds no metadata; README) and two archives for optional installation,
  paths from `/`: `rust-T.tgz` (rustc, cargo, std, 117 MB) and `rust-tools-T.tgz` (150 MB).
  std from the stage-1 sysroot (the same files as stage 2, which bootstrap lifts up; a
  stage-2 build for one target removes the std of the others). On the install CD only on
  request: `RUST_OUT` for 9front-gccwt's `stage-dist.sh` (`rustproto`).
- `tests/tools.sh`: unpacks the archives with 9front's tar in the guest, then rustc/cargo/rustdoc/
  rustfmt/clippy --version, cargo build/run, cargo fmt (--check red before, green after),
  cargo clippy (needless_return), cargo doc, cargo test with unit and doc test.

## arm64: aarch64-unknown-plan9 (Part 14, 2026-10-31)

- **Toolchain**: 9front-gcc now supports `aarch64-pc-plan9` (cross and native, `/arm64/gnu`; NOTES
  there): char signed, no outline atomics, crt*.o without unwind tables. In the
  gcc branch: `arm64/plan9.ld` collects `.init_array`/`.gcc_except_table`, APE's `fenv.h` has
  arm64's values.
- **Target** `aarch64_unknown_plan9.rs`: data layout of aarch64-linux-gnu, `+v8a` without
  `+outline-atomics`, linker `aarch64-pc-plan9-gcc`. `core::ffi::c_char` is `i8` on Plan 9 on every
  architecture (7c, jc; gcc's DEFAULT_SIGNED_CHAR). libc: `jmp_buf` 22 words.
- **std**: `proc.rs` runon/rforkthread/rforkrun in AArch64 (number in R0, argument at 8(SP),
  `SVC #0`; the child gets all registers, R0 = 0: privtop and argument in x26/x27 like APE's
  pthread.c). The stack slots fit in arm64's 32 GiB address space (EVASHIFT 36). Nothing else.
- **Guest**: QEMU virt under TCG, one processor (`tests/guest.sh`: 9front's qemu kernel does not start
  any others, MEMORY.md of the tree); the ext4 disk as virtio-blk-pci 1.0 (`/dev/sdG0`, sdF0
  is u-boot's FAT). Pitfall: the guest boots `arm64/9qemu.u`, which only `build-boot.sh` renews —
  with the old one tokio ran without TCP `shutdown` (EOF, incomplete HTTP requests).
- **Results**: run.sh 13/13, async.sh 5/5 (mio, tokio, http, tlsdns, proc), cargo.sh 5/5
  (cargo built from Linux), rustc.sh 3/3 (rustc native: LLVM with AArch64 backend,
  `objtype=arm64 scripts/build-llvm.sh`, 15 min for rustc and the tools).

## riscv64: riscv64gc-unknown-plan9 (Part 15, 2026-11-10)

- **Toolchain**: 9front-gcc's `riscv64-unknown-plan9` (rv64gc, lp64d, medany, `-mno-relax`, char
  signed), against the tree `../9front-riscv` (branch riscv, rebased onto gcc 82ddeb288). The
  old cross gcc (branch riscv, 21.10.) still had the compat headers in the prefix,
  which have since moved into the tree's APE: `max_align_t`/`lldiv_t` twice. Rebuilt from the branch
  gpu (`objtype=riscv64 PLAN9=../9front-riscv ./build.sh cross`; the old one is in
  `build/riscv64.2026-10-21`); with it also `GTHREAD_USE_WEAK 0` (simultaneous panics).
- **Target** `riscv64gc_unknown_plan9.rs`: data layout of riscv64gc-linux-gnu,
  `+m,+a,+f,+d,+c,+zicsr,+zifencei,-relax` (nobody sets gp), `CodeModel::Medium`, lp64d,
  linker `riscv64-unknown-plan9-gcc`. libc: `jmp_buf` 26 words (sp, pc, s0-s11, fs0-fs11).
- **std**: `proc.rs` runon/rforkthread/rforkrun in RISC-V (number and result in s0,
  argument at 8(sp), `ecall`; the child gets the whole Ureg, s0 = 0: privtop and argument
  in s1/s2 like APE's pthread.c). Stack slots fit (USTKTOP 256 GiB, Sv39).
- `bootstrap.toml`: the cross gccs are now in `9front-gcc/build/` (no longer
  `build-gcc/`); `tests/run.sh` needs `GCC9BUILD=…/9front-gcc/build` for that.
- **cc crate** (1.4.3 cargo, 1.10.0 async tests, 1.11.1 GUI tests; `patches/crates/cc-*.patch`):
  for riscv64, `-march=rv64gc -mabi=lp64d` exists only under Linux, FreeBSD, NetBSD (…), otherwise
  `-mabi=lp64` (soft float): ring's C objects did not link ("can't link soft-float modules with
  double-float modules"). `plan9` into the list.
- Pitfall: after rebuilding the stage-1 rustc (same version line), cargo keeps using the old
  host artifacts (build scripts): E0460/E0463. Delete `target/release` of the test crates and
  `tests/out/cargo-port/release`.
- **Result** (QEMU virt, TCG, 4 harts, `GCC9BUILD=…/9front-gcc/build`): run.sh 13/13,
  async.sh 5/5 (mio, tokio, http, tlsdns, proc), cargo.sh 5/5 (cargo built from Linux,
  `objtype=riscv64 scripts/build-cargo.sh`). No native rustc (LLVM for riscv64).
- **std test suite** (`./x test library/std --target riscv64gc-unknown-plan9 --no-doc
  --no-fail-fast`, `STDTEST_TIMEOUT=2400`): `std-runner.sh` recognizes arm64 and riscv64 (`file`),
  riscv64 boots `../9front-riscv`; `guest.sh -D`: the ext4 disk as virtio-blk-pci 1.0 like
  arm64, in the riscv64 guest `/dev/sdF0` (without u-boot's disk). Fixed on the side: `-D` with a
  directory on another file system copied to `d1/<name>/` (the failed
  `cp --link` had already created `d1`). Result: all 29 integration programs ok
  (`tests/time.rs` 1418 s under TCG); unit program 341 ok, 39 ignored, 1 failed
  (`net::tcp::tests::connect_error`: "connection timed out" instead of "refused"), then
  `fs::tests::concurrent_recursive_mkdir`, `fs::tests::read_large_dir`,
  `net::tcp::tests::close_readwrite_smoke` hung until the limit; only hart 0 of the guest was running
  (~90 %), the other three stood still. Clarified (2026-11-10): the two fs tests do not hang,
  they are just slow under TCG (`read_large_dir` alone 289 s). `close_readwrite_smoke` hung
  on all architectures: since `shutdown(Write)` half-closes, `shutdown(Read)` did nothing on a
  blocking socket. Now in std: all descriptors of the conversation in the process
  (`#p/<pid>/fd`) read the end (`FdState.read_shut`, a dup copies it); `shutdown_smoke`
  runs, `close_read_wakes_up` stays `ignore` (a reader already blocked in the kernel is not
  woken). `connect_error`: 0.0.0.0 went to the default route (SYN without reply);
  the unspecified address is now loopback as under unix and in Go's net on Plan 9.
  In parallel, network tests still hang on the TCP bug under load (below), one after another all green.
  After that unit program riscv64: 346 ok, 0 failed, 38 ignored, in 2558 s (the two
  fs tests alone ~25 min; just within `STDTEST_TIMEOUT=2400` + the 300 s of guest.sh);
  `std-runner.sh` now uses 3600 s for riscv64 when `STDTEST_TIMEOUT` is not set.
- After that (2026-11-10) everything rebuilt: 9front-gccwt (gcc, rebased) amd64/386/arm64 built, the
  cross gccs amd64/386/arm64 from branch gpu against it (old ones in `build/<arch>.2026-11-04`),
  rustc/std were current. run.sh amd64 13/13, riscv64 13/13.

## Rust 1.102: rebase onto upstream, patch series (Part 16, 2026-11-10)

- **rust** `plan9` from `ce37c4ff837` (1.101, 29.10.) rebased onto `origin/main` `6f1c7ee0ec0` (1.102,
  10.11., 1737 commits further): 24 commits, two conflicts. `hop_limit` in the TCP
  and UDP tests is now built by upstream only for unix and Windows: our `ignore` goes away. The
  gitlinks of the submodules at the rebase to upstream's state, then one commit with cargo and
  llvm-project on their `plan9`; the commit that only set cargo's gitlink is dropped.
  `Cargo.lock` from upstream on conflicts, regenerated by cargo. Backup of the old
  branches: `plan9-2026-11-10` in all five repositories.
- **cargo** (3 commits) onto `c07ac2c4d`, **llvm-project** (1 commit) onto `0a5579eb649d`
  (Rust's `rustc/23.1-2026-07-22`), without conflict; **backtrace** and **libc** (0.2.191,
  `libc-0.2` unchanged) stayed.
- **stacker**: rust requires 0.1.24, our patch was for 0.1.21 (unused, "patch was not
  used"): `crates/stacker-0.1.24` with the same change, `patches/crates/stacker-0.1.24.patch`.
- **cargo for 9front**: the new cargo pins libc 0.2.186 and libssh2-sys 0.3.4; against a
  pinned lock a `[patch]` of another version does not apply (libz-sys: no `off_t`).
  `cargo update --config cargo-plan9.toml -p libc -p libssh2-sys` in `rust/src/tools/cargo`
  (the lock change stays local, as before).
- **Patch series**: `patches/{rust,cargo,backtrace,llvm-project,libc}` (`git format-patch`,
  `BASE`: upstream URL and base), `scripts/rust-patches.sh diff|apply`; checked: applied to the
  base they yield exactly the trees of `plan9`.
- Stage 1 builds without new warnings (the 31 "unexpected cfg plan9" belong to the stage-0 compiler).
  Tests: run.sh amd64, 386, arm64, riscv64 13/13 each; cargo.sh 5/5 and async.sh 5/5 (amd64).
  arm64: `9qemu.u` was older than the newly built kernel (boot-cli refuses): build-boot.sh.
- On the side: the default paths of the scripts to 9front-gcc's `build/` and `out/` (`build-gcc`
  no longer exists, `out-gcc` was outdated); the native gccs for 9front (amd64, 386, arm64,
  riscv64) rebuilt in `out/` against the current trees.
- **Native (2026-11-11)**: LLVM for 9front (amd64, 386, arm64) rebuilt from the rebased
  llvm-project (23.1.5), then rustc 1.102 stage 2 per host. A fresh LLVM build had no
  `bin/llvm-config` (bootstrap: "No such file"), and llvm-native-tools did not have the libraries
  that `llvm-config-plan9.sh` links: `build-llvm.sh` builds them too and makes llvm-config at the end.
  `tests/rustc.sh` amd64, 386, arm64 3/3 each; `tests/cargo-build.sh` amd64, 386, arm64 10/10 each
  (proc macros, serde-derive); cargo for 386, arm64, riscv64 rebuilt, `tests/cargo.sh` 5/5 each.
- **std for 386 and arm64 in the stage-1 sysroot (2026-11-11, for 9front's cargo.sh):** the
  target list in `bootstrap.toml` now also has `i686-unknown-plan9` and `aarch64-unknown-plan9`.
  Pitfall: `./x build library --stage 1 --target A,B` cleans the sysroot
  (`build/host/stage1/lib/rustlib`) down to exactly A and B — the std of `x86_64-unknown-plan9`
  was gone afterwards; always give all targets (or take the list from `bootstrap.toml`).
- **386: stack slots took the whole lower address space (2026-11-11, found with wsys in the
  9pc guest, `/proc/N/segment`):** 64 slots of 64 MB from 1 GiB below the stack occupied on 386
  everything from 0x2100000 to 0x7ff00000 (31 of them, up to the overlap with bss) — including segment(3)'s
  shared region (SHAREDBASE 0x6fe00000: wsys could not map in Wayland pools,
  "segments overlap") and the heap kept 31 MB. Now on 32 bit 16 slots of 8 MB directly
  below SHAREDBASE (USTKTOP/2, from the position of the stack), ~1.5 GB heap; 64 bit unchanged.
  `tests/run.sh` 386 13/13.

- **cargo on the nightly's own crates (2026-11-12):** with the sources from the archive
  (nightly-2026-11-11) cargo's `Cargo.lock` stays as published; our crates follow it instead of
  pinning the lock to ours (the earlier `cargo update -p libc -p libssh2-sys`, a local step lost
  with the fresh archive: "patch was not used", libz-sys without `off_t`). For cargo's plan9
  build only libc (0.2.186) and libssh2-sys (0.3.4) differed: `crates/libc-0.2.186` is our
  libc series (made on 0.2.191, std's version) applied there unchanged, src/plan9.rs identical;
  `crates/libssh2-sys-0.3.4` the 0.3.2 change (0.3.2 dropped, nothing used it). tokio,
  hyper-util and home 0.5.14 of the lock are not built for plan9. cargo still adds the
  dependencies of our curl-sys (rustls) to its lock; no locked version changes.

## Open

As of 2026-10-31: amd64 and 386 cross and native (std, rustc, cargo, proc macros as programs,
file locks), std/core/alloc test suites in the guest green except the TCP tests under load.
- **async**: TCP keepalive in std is missing; a caught note interrupts blocked calls
  (Part 12).
- **9front kernel** (not from the port, 9front-gccwt MEMORY.md): TCP `connect` over loopback hangs
  under load (`tools/csstress`); 2026-11-10 also on the unmodified `front`, kernel and
  userland built with kencc.
- **arm64** (Part 14): std test suite not run; the guest has only one processor.
- **Tools**: the doc tests of std never run (`--no-doc`); rustdoc and clippy-driver
  each contain the whole compiler (together with rustc ~490 MB; one program for all three
  would be ~170 MB).
- APE's copy procs: after `close` with a running read their buffer stays (leak, cargo).
- acid does not read the ELF symbols of the gcc port (tools/pstack + addr2line instead).
- What Plan 9 does not have (stays so, tests `ignore`d with reason): symlinks, hard links, setting
  atime, reading back TTL/keepalive, linger.

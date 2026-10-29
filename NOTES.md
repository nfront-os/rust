# 9front-rust — Log

Ziel: Rust für 9front, zuerst als Cross-Compiler (Linux → 9front amd64, core/alloc/std),
std **nativ auf Plan 9s libc** (`libc.h`, `/amd64/lib.gcc/libc.a`), nicht auf APE.
Später: rustc/cargo nativ auf 9front, 386.

## Bäume (je eigenes git-Repo, Branch `plan9`)

- `rust/`: rust-lang/rust main (1.101-dev). `bootstrap.toml` (Vorlage: `bootstrap.toml.example`,
  `$GCC9` = `../9front-gcc`): LLVM aus Rusts CI (`download-ci-llvm`, keine LLVM-Änderung nötig),
  `deny-warnings = false` (std hat unabhängig vom Port eine unerfüllte Lint-Erwartung,
  `ineffective_unstable_reexports` in `lib.rs`; eigene Warnungen im Log prüfen).
- `libc/`: rust-lang/libc, Branch `plan9` von `libc-0.2` (= 0.2.191, Version von std).
  `library/Cargo.toml` patcht `libc` auf `../../libc`.
- `llvm-project/`: noch unbenutzt (für die native Phase).

Bauen: `cd rust && ./x build --stage 1 library --target x86_64-unknown-plan9`
(Stage-1-Compiler ~4 min, std ~1 min). Programm:
`PATH=../9front-gcc/build/amd64/cross/bin:$PATH rust/build/host/stage1/bin/rustc --edition 2024 --target x86_64-unknown-plan9 x.rs`.
Im Gast: `tests/guest.sh -f prog... -- 'rc-Zeile' ...` (über `../9front`s `boot-cli.sh`).

## Target x86_64-unknown-plan9 (Teil 1)

- `Os::Plan9`, keine Familie (`cfg(unix)` aus). LLVM-Triple `x86_64-unknown-plan9`: unbekanntes
  OS = ELF + System-V-ABI, wie 9front-gcc. Statisch, `RelocModel::Static`, kein PIE,
  `has_thread_local: false`, Stack-Probes inline, vorerst `panic=abort`.
- Linker = 9front-gccs `x86_64-pc-plan9-gcc` (Linker-Skript `/amd64/plan9.ld` vom Treiber):
  `-nodefaultlibs` (sonst APE: `-lap`), dafür selbst `-Wl,--wrap=main` und spät `-lgcc -lc`.
  Mit `-nodefaultlibs` bleiben `crtbegin.o/crtend.o` (Unwind-Tabellen über `.init_array`).
  libgccs `__wrap_main` (9front-gcc) stellt die FPU wie POSIX (9fronts Kernel trappt sonst bei
  1.0/0.0) und ruft `.init_array`, dann `main`.
- `bootstrap`: Target in `STAGE0_MISSING_TARGETS`; `std/build.rs` kennt `plan9` (sonst
  `restricted_std`).

## libc-Crate (`src/plan9.rs`)

- Typen wie gcc sie sieht: `long` = 64 Bit, `ulong` = 32 Bit (u.h unter `__GNUC__`),
  `malloc(ulong)` → Allokationen ≥ 4 GiB schlagen fehl. Kernel-Semaphoren sind `s32int`
  (Port-Fix im Kernel), `<libc.h>` sagt noch `long*` → Rust `*mut c_int`.
- `await` ist in Rust ein Schlüsselwort → `await_` mit `link_name`.
- mem*/strlen mit `size_t` (rustc prüft die Laufzeit-Symbole gegen ihre kanonische Signatur).

## std (`library/std/src/sys/*/plan9.rs`, `pal/plan9/`)

- **Threads = Procs** `rfork(RFPROC|RFMEM|RFNOWAIT)`. RFMEM teilt alles außer dem
  Stack-Segment; Rust gibt Verweise auf Stack-Daten an andere Threads (scoped threads, mpmc
  mit Paketen auf dem Stack) → jeder Thread, auch `main`, läuft auf geteiltem Speicher.
  `main`: libgccs `__wrap_main` ruft das schwache `__plan9_mainstack` (Haken aus 9front-gccs
  pthreads), std definiert es (`pal::init` referenziert es, sonst zöge der Linker das Objekt
  nicht aus dem rlib) und endet mit dem Ergebnis über `exits`. Vorlage für den ganzen Mechanismus:
  9front-gcc `compat/lib/pthread.c` (rfork im Asm, Kind erbt nur R14/R15, läuft unter
  `PRIVTOP` auf seiner Kopie des Stack-Segments, beendet sich dort → Stack frei für `join`).
- **Stacks mit Lücke** (Stacküberlauf blieb mit malloc-Stacks unbemerkt): jeder Stack ein
  eigenes Segment (`segattach(SG_CEXEC, "memory", va, 64 MiB)`, Seiten erst bei Zugriff), mit
  1 MiB ungemappt dazwischen. Ein Segment, das ein Proc später anhängt, sehen die anderen nicht
  → `main`s Proc hängt 64 Slots an, bevor es Threads gibt (1 GiB unter dem Stack-Segment, nach
  unten). Freigabe: `segfree`. Mehr Threads oder > 64 MiB: malloc-Stack ohne Lücke. Überlauf:
  Kernel `suicide: sys: trap: fault write addr=…`, Proc Broken (Notes laufen auf dem Stack, der
  ist hin → keine Rust-Meldung möglich). 9front erlaubt 250 Benutzersegmente (NSEG = SEG1+250).
- **Achtung fork**: Slot-Segmente sind SG_SHARED, auch ohne RFMEM geteilt → ein `rfork` ohne
  RFMEM (Command) muss das Kind sofort vom Stack nehmen (wie bei Threads).
- **Futex** aus Semaphoren: Warteschlange pro Hash-Bucket (Spinlock), jeder Thread wartet auf
  seiner eigenen Semaphore (`ThreadData`, libc-malloc, geteilt). Timeout `tsemacquire` (ms,
  aufgerundet), Note unterbricht → spurious wakeup. Darauf std's futex-Mutex/Condvar/RwLock/
  Once/Parker.
- **TLS**: ein `privalloc`-Slot (liegt auf dem privaten Stack-Segment) → `ThreadData` → Tabelle
  mit 1024 Keys. Kind erbt die Kopie des Slots → als Erstes nullen. Destruktoren am Thread-Ende
  selbst (Guard „retained“ wie Xous/Hermit).
- **Fehler** sind Zeichenketten: `io::Error::last_os_error()` → `Error::new(kind, errstr)`,
  `kind` aus den Kernel-/Dateiserver-Meldungen (`port/error.h`). `raw_os_error()` = None.
- **Ende**: `exit(0)` → `exits(nil)`, sonst `exits("<code>")`. Andere Threads per
  `/proc/n/ctl kill`; endet ein Thread das Programm, bekommt `main`s Proc den Status als Note
  (unbehandelt → endet mit dem Text als Status), damit der Elternprozess ihn sieht.
  `abort` = Status `abort`. Note „sys: write on closed pipe“ wird ignoriert (wie SIGPIPE).
- Zeit: `Instant` = `uptime()` (ns seit Boot), `SystemTime` = `nsec()`. Zufall `/dev/random`.
  Parallelität: Zeilen von `/dev/sysstat`. `sleep` in ms, aufgerundet.
- malloc liefert 8-Byte-Ausrichtung (Bhdr 8 + Tag 8?) → größer über `mallocalign`.

## Tests

- `tests/nostd.rs` (core, libc von Hand), `tests/std1.rs` (args, scoped threads, Mutex unter
  Last, Condvar/Timeout, Barrier, RwLock, Kanäle rendezvous/gepuffert, TLS + Destruktor,
  sleep, SystemTime, HashMap, Float-Format), `tests/overflow.rs` (Überlauf in main und Thread
  → Broken): laufen im Gast (2026-10-29).

## Offen

- fs, env, paths, process (fork vom Stack weg!, wait aus anderen Threads: `/proc/n/wait`),
  pipe, net (`/net`, dial/announce, DNS über `/net/cs`), `os::fd`.
- `panic=unwind` (libgccs Unwinder, crtbegin registriert `.eh_frame`).
- Testlauf als Skript, Tests der std-Testsuite im Gast; 386; nativer rustc.

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

## std Teil 2: fs, env, Prozesse, Netz, Unwinding (2026-10-29)

- **fs**: `open`/`create` (create auf vorhandene Datei kürzt → `create` ohne truncate = open, sonst
  `create|OEXCL`, Wettlauf → wieder open), stat(5)-`Dir` als FileAttr (uid/gid/muid, qid über
  `os::plan9::fs::MetadataExt`), `dirread` stückweise. Kein Append-Modus in open → seek ans Ende
  vor jedem write. rename = wstat des Namens nur im selben Verzeichnis (Vergleich der qids der
  Eltern), ersetzt nichts → Ziel vorher entfernen; über Verzeichnisse: Datei kopieren (Rechte,
  Zeiten) + entfernen, Verzeichnisse: CrossesDevices. Keine Links, Locks, fsync (Ok).
- **`fd_view`**: std nimmt an, dass `File`, `TcpStream`, … genau ein `RawFd` groß sind → Zusatzzustand
  (Append, Timeouts, UDP-Peer) in einer Tabelle pro Deskriptor (`sys::fd::FdState`), gelöscht in
  `OwnedFd::drop`, gesetzt von jedem, der einen Deskriptor macht.
- **close-on-exec**: pipe(2)/dial geben keins; `open("#d/N", mode|OCEXEC)` liefert eine Kopie mit
  OCEXEC (devdup gibt den Kanal selbst zurück, fd2path bleibt der alte Name). `dup(a, b)` löscht
  OCEXEC an b (`f->flag[fd] = 0`).
- **env**: `/env`-Dateien; inneres `\0` (rc-Listen) bleibt, `split_paths`/`join_paths` trennen an `\0`
  (`$path`). `current_exe`: argv[0], ohne `/` über `$path` (Plan 9 kennt den Programmnamen nicht).
- **Prozesse**: `rfork(RFPROC|RFFDG|RFREND[|RFENVG|RFCENVG])` per Asm (`rfork_run`), Kind sofort auf
  seinem Stack-Segment (die Slots sind auch ohne RFMEM geteilt!), keine Allokation/Locks im Kind
  (Speicherkopie, malloc-Lock evtl. gehalten). Exec-Fehler über OCEXEC-Pipe. Programmsuche in
  `$path` wie rc. Warten: jeder Thread liest `/proc/<erzeuger>/wait` (einer pro Erzeuger, Ergebnisse
  für andere in einer Tabelle) → `wait` aus jedem Thread. Grenze: Kind eines beendeten Threads ist
  verwaist (Plan 9 gibt Kinder nicht weiter). `try_wait`: Plan 9 hat keine Zombies → `/proc/pid` weg
  = beendet. ExitStatus = Waitmsg-Text (Copy → `[u8; ERRMAX]`), `code()` = Zahl nach „: “.
- **Netz**: dial/announce/listen/accept. fd2path eines announce-ctl ist `/net/tcp/clone` → ctl
  beim Namen neu öffnen. UDP mit `headers` (52 Byte). Timeouts per `alarm` (Note „alarm“ wird im
  Handler geschluckt). `shutdown(Write)` = ctl `close`. Namen über `/net/cs`. Im Test-Gast fehlt
  Loopback: `ip/ipconfig loopback /dev/null 127.1`.
- **Unwinding**: `panic=unwind` Standard; libgccs Unwinder (unwind-dw2-fde in libgcc.a, crtbegin.o
  registriert `.eh_frame`), `unwind`/`panic_unwind`/personality wie unix. Backtraces: Submodul
  `library/backtrace` (Branch `plan9`): libunwind-Trace, gimli liest `/proc/<pid>/text`.

## cargo für 9front (Teil 3, 2026-10-29)

`scripts/build-cargo.sh` baut cargo (`rust/src/tools/cargo`, Branch `plan9`) für
x86_64-unknown-plan9; `tests/cargo.sh` testet im Gast: `--version`, `new` (libgit2), `search`
(crates.io-API: curl + rustls), `fetch` (Index, Downloads, Entpacken, sqlite), alle ok.
Ohne rustc auf 9front gibt ein rc-Skript die Antworten des Stage-1-rustc (`-vV`, `--print`).

- **C-Bibliotheken über APE: ape9** (`scripts/ape9.sh`, `ape9/`): libap, libbsd, 9front-gccs
  libapecompat (ohne `pthread.o`, das wie std `__plan9_mainstack` definiert) mit **jedem
  definierten Symbol umbenannt** `__ape_<name>` (objcopy `--redefine-syms`, 1000+ Symbole);
  die C-Bibliotheken der Crates genauso: `scripts/ape9-ar` ist das `ar` des cc-Crates und
  benennt das Archiv nach. So treffen APEs `open`, `read`, `malloc`… nicht auf libcs. APEs
  `malloc.o` fliegt raus (malloc/free/realloc = libcs, ein Heap; aligned_alloc &c. in
  `ape9init.c` über `mallocalign`). `ape9init.c` (aus `.init_array`) tut, was APEs `_main`
  täte: `_tos`, errno pro Proc (ein `privalloc`-Slot), `_envsetup` (aber ohne `_NOTIFY`: std hat
  den Note-Handler; per objcopy auf `__ape9_nonotify`), APEs stdio beim Ende leeren (libcs
  `atexit`: Rust endet über `exits`). Link: `-u __ape9_init`, Gruppe der libape9-*, dann `-lc`
  nochmals (ape9 steht hinter `-lc`).
- **APEs `select` ist unbrauchbar** in einem Rust-Programm: es forkt Kopier-Procs (ohne
  RFMEM) auf dem laufenden Stack, der hier geteilt ist (Stack-Slots sind SG_SHARED), hängt
  seinen Puffer-Segment erst beim ersten Aufruf an (andere Procs sehen es nicht → Fault),
  nur ein Wartender. Ersetzt durch `ape9/ext/buf.c` (`_startbuf`, `_readbuf`, `_closebuf`,
  `select`, `poll`): Kopier-Procs RFMEM|RFFDG über std (`__plan9_rforkproc`: eigenes
  Stack-Segment, als Thread registriert → endet mit dem Programm), Puffer aus malloc,
  beliebig viele Wartende auf eigenen Semaphoren. Der Kopier-Proc schließt alle fremden
  fds (RFFDG kopiert die geteilte fd-Tabelle aller Threads → hielt z. B. den Server-Socket
  eines anderen Threads offen, kein EOF), mit rohen Aufrufen (`_fdinfo` ist geteilt).
- ape9/ext/posix.c: fsync, getsid/getpgid, utimes, getpwuid_r/getpwnam_r, fchown (no-op),
  posix_fallocate (ftruncate), **getsockopt** (APEs gibt -1 ohne errno → curl las altes
  `errno` = EAGAIN als Verbindungsfehler; SO_ERROR=0). ape9/ext/pthread.c: nur Mutexe
  (Drepper + Semaphore, rekursiv pro Proc) für sqlite. ape9/include: Ergänzungs-Header
  (`#include_next`): poll.h, unistd.h, sys/socket.h (HAVE_SOCK_OPTS, fd_set), netinet/in.h …
- libc-Crate: APEs POSIX-Typen (off_t, fd_set, sockaddr, …) und errno-Werte (APE: EINVAL=12,
  ENOMEM=23!) für die Bindings der C-Bibliotheken.
- **Crates** (`patches/crates/*.patch`, `scripts/crates-patch.sh apply` → `crates/`,
  `cargo-plan9.toml` = `[patch.crates-io]`; `cargo update -p libc` nach Patch-Änderungen):
  getrandom 0.2/0.3/0.4 (`/dev/random`), ring (Linux-ABI-Liste: SysV + ELF → Asm), rustls-
  native-certs (`/sys/lib/tls/ca.pem`), rustls-platform-verifier, curl (socket über
  `__ape_socket`, kein socket2; curl-sys mit **rustls** statt OpenSSL, ring statt aws-lc),
  curl-sys (kein Threaded-Resolver, keine Unix-Sockets/socketpair, `CURL_CA_BUNDLE`),
  libgit2-sys (ohne TLS/SSH — git über curl via git2-curl —, ohne Threads, poll, NO_MMAP),
  libssh2-sys leer, libsqlite3-sys (ohne mmap/WAL/Extensions/Worker; Locks mit
  „unix-dotfile“ per SQLITE_EXTRA_INIT, APEs fcntl hat keine Record-Locks), git2, tar
  (wie wasm32, aber Rechte setzen), url (file paths), filetime, home, is_executable,
  gix-sec (`/dev/user`), gix-config-value (`/usr/<name>`). Umgebung: `LIBZ_SYS_STATIC=1`
  (APEs zlib.h ist alt).
- cargo selbst: cargo-util (exec_replace = spawn+wait, read2 mit zwei Threads, Pfade),
  cargo-credential (`/dev/cons`), Terminal ohne Escapes, job (Note-Gruppe), flock (Plan 9 hat
  keine Locks → wie ein Dateisystem ohne), FileId, `$path` statt `PATH`.
- std-Fix: `Path::is_absolute` (Plan 9 war nicht in der unix-artigen Liste).
- **CA-Wurzeln**: 9front prüft TLS mit Thumbprints, hat kein CA-Bündel: cargo erwartet
  `/sys/lib/tls/ca.pem` (oder `http.cainfo`). Für die Tests das des Hosts.
- Nach jedem std-Neubau: `rm -rf tests/out/cargo-port` (sonst „possibly newer version of
  crate std“), und std immer für beide Targets bauen (sonst fehlt eins im Sysroot).

## Tests

- `tests/run.sh`: baut `tests/*.rs`, läuft im Gast, eine Zeile pro Test; Stand: alle 5 ok.
- `tests/nostd.rs` (core, libc von Hand), `tests/std1.rs` (args, scoped threads, Mutex unter
  Last, Condvar/Timeout, Barrier, RwLock, Kanäle rendezvous/gepuffert, TLS + Destruktor,
  sleep, SystemTime, HashMap, Float-Format), `tests/overflow.rs` (Überlauf in main und Thread
  → Broken): laufen im Gast (2026-10-29).

## Offen

- cargo nativ braucht rustc nativ (LLVM über 9front-gccs g++). Offen bei cargo: Datei-Locks
  (DMEXCL?), Kopier-Procs-Puffer nach `close` mit laufendem read bleiben stehen (Leck).
- cargo (Linux, cross): host-cargo + `RUSTC=rust/build/host/stage1/bin/rustc`, Linux-std im Stage-1
  (`./x build --stage 1 library --target x86_64-unknown-linux-gnu`), `.cargo/config.toml` mit
  `linker`. `tests/crates` (regex, serde, serde_json) noch nicht gebaut. Crates von crates.io mit
  `libc`: das crates.io-libc kennt plan9 nicht → `[patch.crates-io] libc = { path = …/libc }`.
- cargo nativ: rustc + LLVM nativ (g++ von 9front-gcc), dann cargos Abhängigkeiten (curl+TLS,
  git2/gix, sqlite, flock-Ersatz, jobserver ohne cfg(unix)).
- 386; std-Testsuite im Gast; Datei-Locks (DMEXCL?); `peek`, nonblocking.

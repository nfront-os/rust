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

## APE neben der nativen libc, ohne ape9 (Teil 4, 2026-10-29)

Der Nutzer wollte, dass C aus Crates ohne ape9-Umbenennung linkt. Umgekehrt gelöst: die
**native libc** wird umbenannt, APE bleibt, wie es ist.
- 9front (Branch `gcc`): compat von 9front-gcc ist in APE (`sys/src/ape/lib/ap/gnu`,
  `sys/src/ape/lib/musl`, Header unter `__GNUC__`); `libp9.a` (`scripts/build/libp9.sh`, Post-
  Schritt von libc): libc.a mit allen Namen als `__p9_*`; `ap/gnu/native.c`: `__ape_attach`
  (Konstruktor 101: `_tos`, errno/`_privates` in privalloc-Slots, `_envsetup` ohne Notes),
  `__ape_exit`; leere `libm/libpthread/librt/libdl.a`. 9front-gcc linkt nur noch `-lbsd -lap`
  (kein libapecompat), Bau in `../9front-gcc/build-gcc` gegen `../9front-gccwt` (`GCC9BUILD`),
  bis `gpu` den Branch `gcc` mergt.
- libc-Crate: jede native Funktion `#[link_name = "__p9_…"]`. Target: Einstieg `__p9__main`,
  `--start-group -lp9 -lbsd -lap -lgcc --end-group`. std: `malloc`, `free`, `realloc`,
  `aligned_alloc`, `memalign`, `posix_memalign` (16 Byte, auf libcs mallocalign; APEs malloc.o
  kommt nicht in den Link: ein Heap), `__ape_exit` in `end_program`.
- C/C++ der Crates: normale APE-Übersetzung mit `x86_64-pc-plan9-gcc`/`ar`; APEs pthreads
  laufen auf std-Threads (`__plan9_thread_spawn`), select/poll über APEs neue Pufferung.
- Gefunden: APEs `uintptr_t` war `unsigned long long`, `size_t` `unsigned long` (curl ↔
  rustls-ffi; ape9s `-Wno-error=incompatible-pointer-types` hatte das verdeckt): für gcc jetzt
  die Typen des Compilers. Nach einem rustc-Neubau std für **beide** Targets in einem Lauf
  bauen (`--target x86_64-unknown-linux-gnu,x86_64-unknown-plan9`), sonst fehlt eins.
- **Regel für Crate-Ports und eigenen Code:** native Plan-9-Aufrufe nur über das libc-Crate
  oder mit `#[link_name = "__p9_…"]`. Ein eigenes `extern "C" { fn open(…) }` landet still bei
  APEs `open` (andere Flags: OCEXEC 32 ist dort etwas anderes); so war es in den getrandom-Ports,
  die Tests liefen trotzdem. `scripts/crates-patch.sh diff` meldet solche Deklarationen.
- Tests: `tests/run.sh` 7/7 (neu `cmain`, `cxxmain`: C und C++ aus `tests/c`), `tests/cargo.sh`
  5/5 (version, new, search, fetch, refetch).

## rustc auf 9front (Teil 5, 2026-10-30)

`./x build --stage 2 --host x86_64-unknown-plan9 --target x86_64-unknown-plan9 compiler/rustc`
baut rustc für 9front: statisch, LLVM (build/llvm-plan9) eingebaut, 575 MB mit Debug-Infos.
- LLVM gegen das neue APE: `SIGTRAP` fehlte in APE; CMakes FindThreads riet `-lpthreads`
  (seine Proben können nicht linken): Threads in libap gesagt. bootstrap hatte das Submodul
  llvm-project auf Rusts Commit zurückgesetzt (`submodules = false`, Gitlink auf plan9).
- bootstrap: ein Target mit eigenem `llvm-config`, das auf dem Bauhost läuft
  (`scripts/llvm-config-plan9.sh`), gibt `rustc_llvm` dieses statt des Hosts.
- Compiler: `rustc-main` nimmt `rustc_driver_impl` direkt (keine dylib); `dll_path` ist das
  Programm selbst (`current_exe`); memmap liest die Datei (wie wasm); kein ctrlc;
  `path_to_c_string` über `os::plan9`; libc, libloading (keine dylibs: Öffnen scheitert),
  getrandom 0.3.3 aus `crates/` per `[patch.crates-io]`.
- **Keine Proc-Macros** in einem rustc auf 9front (dylibs). Plan: Proc-Macro-Crates als kleine
  Programme, die Brücke (`Buffer`-Nachrichten wie bei `cross-thread`) über eine Pipe.
- Linker auf 9front: der native gcc von 9front-gcc (`out-gcc`, `x86_64-pc-plan9-gcc`), dazu
  `/amd64/include`, `/amd64/lib.gcc` (mit `libp9.a`), `/amd64/plan9.ld`, `/sys/include` des
  Baums mit dem neuen APE. Beim nativen gcc fehlte `getpagesize` (libbacktrace nimmt jetzt
  mmap), in eigener Objektdatei (libiberty hat ein eigenes).
- `tests/rustc.sh`: rustc --version, übersetzt `tests/std1.rs` im Gast und führt es aus:
  alle ok (MEM=3072). `tests/guest.sh -d /gast/dir=hostdir` kopiert Verzeichnisse.

## cargo build auf 9front (Teil 6, 2026-10-30)

`tests/cargo-build.sh` (rustc, cargo, nativer gcc, Header/Bibliotheken des gcc-Baums im Gast;
`CMDWAIT` hoch: boot-cli wartet sonst 60 s pro Zeile): `cargo new` + `cargo build` + Lauf ok,
`itoa` von crates.io ok. Gefunden: jobserver 0.1.34 fiel auf seinen wasm-Stub (`configure`
→ Panik) → Jobserver nur im Prozess; 9fronts Kernel prüft die Pufferadresse auch bei 0 Bytes
(leerer Slice: 0x1) → std `buf_ptr`; rustc inkrementell braucht Locks → plan9: ohne Sperre
(wie cargo); rustc findet seine Sysroot über den eigenen Pfad: als `/bin/rustc` (bind) wäre
das `/` → auf plan9 Rückfall auf `/$cputype/rust` (rustc_session::filesearch).
- serde (Build-Skript), Release-Bau: grün (`tests/cargo-build.sh` 6/6, 4 CPUs, ext4-Image,
  ~2 min). Gefunden auf dem Weg: libstdc++ ohne TLS → `call_once` verklemmt mit mehreren CPUs
  (LLVMs Pass-Initialisierung; 9front-gcc: HAVE_TLS über emutls); ext4srv `truncate` auf gleiche
  Länge und `qid.vers` ohne Änderung bei Schreibzugriffen (der Kernel startete neu gebaute
  Programme mit altem Text: invalid opcode) — beides im 9front-Branch gcc behoben; Gäste mit
  4 CPUs (`SMP`), Bäume auf einem ext4-Image (`guest.sh -D`). Früher: im zweiten Diagnoselauf war rustc `Ready` (rechnet), cargos `PREAD`-
  Procs sind APEs Kopier-Procs der curl-Verbindungen. Eher sehr langsam als Deadlock: messen
  (Zeit für serde im Gast, `-Z time-passes`), Allokator (alles über mallocalign 16?) prüfen.
  Log: `logs/cargo-dbg-serde-hang.log`.
- **Datei-Locks mit DMEXCL** (2026-10-30): `std::os::plan9::fs::lock_file(path, wait)` gibt
  einen `FileLock`: die Datei allein offen als exklusiv benutzbare (DMEXCL) Datei; ein zweites
  Öffnen scheitert („file is locked“ gefs/cwfs/ramfs, „file locked“ hjfs → `WouldBlock`),
  `wait` wiederholt alle 100 ms. Neue Sperrdateien entstehen mit DMEXCL (create), alte bekommen
  es per dirfwstat und werden neu geöffnet. Keine geteilten Locks (geteilt = exklusiv).
  - open(5): der Server darf die Sperre einer unbenutzten Datei brechen (cwfs `TLOCK` 5 min,
    hjfs `EXCLDUR` 300 s, danach scheitern dort sogar Lesen/Schreiben des Halters; gefs zählt
    nur Referenzen). Wie upas' `trylock` liest ein Thread des Locks jede Minute ein Byte;
    Drop beendet ihn (Condvar) und schließt dann die Datei (Hinweis eines Nebenagenten).
  - ext4srv von 9front kennt DMEXCL nicht (Bit bleibt nicht): nach einem wstat-Versuch gibt
    `lock_file` die Datei ohne Sperre zurück (wie upas: „live without the lock“), kein Hänger.
    Im gcc-Branch (9front-gccwt) hält ext4srv DMEXCL jetzt (Inode-Flag UNRM, ein Öffnen zur
    Zeit, „file is locked“): die cargo-Gäste (`PLAN9=../9front-gccwt`) sperren wirklich.
  - std's `File::lock*` bleiben `Unsupported` (ein Lock braucht hier das Öffnen selbst).
  - cargo (`util/flock.rs`, `mod plan9`): nur cargos eigene Sperrdateien (`.cargo-lock`,
    `.cargo-build-lock`, `.cargo-artifact-lock`, `.package-cache`, `.package-cache-mutate`,
    `.lock` der Bau-Einheiten) bekommen DMEXCL; Datendateien (`Cargo.lock`,
    `credentials.toml`, `.crates.toml`, Pakete) nicht: DMEXCL bliebe an ihnen, und cargo öffnet
    `Cargo.lock` im selben Prozess zweimal. Ein Prozess teilt seine Locks einer Datei (eine
    Tabelle `HELD`), sonst sperrte er sich selbst; warten außerhalb der Tabelle, „Blocking“ wie
    sonst. Nicht beschreibbar → ohne Sperre.
  - rustc (`flock/plan9.rs`): die vorher geöffnete Datei zu, dann `lock_file(p, false)`.
  - Test: `tests/lock.rs` in `tests/run.sh` (ramfs: exklusiv, Warten bis Drop, alte Datei wird
    DMEXCL, Drop sofort; `/tmp`: wie der Server es hat); von Hand auf ext4srv (gcc-Branch): exklusiv wie ramfs, e2fsck ok.
- `abort` aus C: `__plan9_abort` in std beendet die anderen Procs, der abbrechende endet mit
  `ud2` und bleibt Broken (APE `gen/abort.c`). Getestet (`cexit-abort`). acid liest die
  ELF-Symbole des gcc-Ports nicht ("no symbol information"): PC aus `/proc/N/status`/acid,
  auf dem Host `addr2line -e binary PC`.

## Proc-Macros als Programme (Teil 7, 2026-10-30)

Plan 9 hat keine dylibs; ein proc-macro-Crate für einen plan9-Host wird ein statisches
Programm, rustc spricht mit ihm über stdin/stdout (`library/proc_macro/src/bridge/remote.rs`).
- Die Brücke tauscht ohnehin nur `Buffer` (Handles als Zahlen, Symbole als Strings), wie die
  cross-thread-Strategie; `RemoteStrategy` schickt sie durch eine Pipe: `R` (Index, Eingabe),
  das Programm fragt mit `D` zurück (rustc: `Dispatcher::dispatch`, Antwort `r`), Ergebnis `O`.
  Namen und Arten der Makros stehen in den Metadaten; das Programm meldet nur die Anzahl (`L`).
  Verschachtelte Läufe (expand_expr) bekommen eine zweite Instanz. stdout des Makros → stderr.
- Compiler: Harness erzeugt `#[rustc_main] fn main() { serve(_DECLS) }`; `entry_fn` für plan9-
  proc-macro; Link als `StaticNoPicExe`, keine Exportliste, `--undefined` des Metadaten-Symbols
  (sonst wirft `--gc-sections` `.rustc` weg); `invalid_output_for_target` erlaubt es;
  `rustc_metadata` lädt mit `remote::load`. `Client` hat auf plan9 ein Feld `remote`.
- Beim Cross-Bau von rustc baut bootstrap Proc-Macros auch fürs Ziel (dual proc macros): die
  sind jetzt diese Programme (nur ihre Metadaten werden gebraucht).
- `tests/cargo-build.sh` 9/9: eigenes derive/attribute/bang-Makro, verschachtelte Makros
  (`expandexpr`: `TokenStream::expand_expr` auf einen Makro desselben Crates, drei Ebenen tief,
  also vier Instanzen des Programms gleichzeitig; ein eingebauter Makro `concat!` darüber),
  serde-derive + serde_json (~5 min, 4 CPUs). Auf Linux dieselben Makros: gleiche Ausgabe.
  `macrocrash` (10/10): ein innerer Makro, der panict (wie auf Linux: ein Fehler „proc macro
  panicked“, „message: boom inside“, die Instanz läuft weiter) und einer, der mit `exit`
  endet (Fehler „proc-macro program failed“; die tote Instanz kommt nicht in den Vorrat, der
  nächste Lauf bekommt eine neue); die Makros danach expandieren richtig (`assert!`s halten).

## 386: i686-unknown-plan9 (Teil 8, 2026-10-30)

Cross (Linux → 9front 386), Entscheidung: SSE2 wie Rusts i686-Targets (`cpu = pentium4`); der
Baum selbst ist `-march=pentium` (x87), die C-ABI ist dieselbe. 9pc lädt MXCSR mit 0x1f80
(SSE-Ausnahmen maskiert), libgccs `__wrap_main` setzt das x87-Steuerwort wie auf amd64.
- Toolchain: `objtype=386 BUILD=$PWD/build-gcc OUTDIR=$PWD/out-gcc PLAN9=../9front-gccwt
  ./build.sh cross` in 9front-gcc (gegen den 386-Baum, `CC=gcc ARCH=386 build.exe -D` in
  9front-gccwt; danach baut dort der nächste amd64-Lauf alles neu: gemeinsame Objekte).
- Target `rustc_target/.../i686_unknown_plan9.rs`: Datenlayout von i686-unknown-linux-gnu
  (System V i386: double und long long in Structs 4-Byte-ausgerichtet, wie gccs), sonst
  `base::plan9`. `bootstrap.toml`: `[target.i686-unknown-plan9]` (Vorlage), nicht in
  `build.target`: `./x build --stage 1 library --target i686-unknown-plan9` ausdrücklich.
  Nach jedem Compiler-Neubau std für alle Targets bauen (sonst fehlt eine im Sysroot,
  E0463): `--target x86_64-unknown-linux-gnu,x86_64-unknown-plan9,i686-unknown-plan9`.
- std: der Assembler in `pal/plan9/proc.rs` für x86 (`runon`, `rforkthread`, `rforkrun`):
  Systemaufruf `INT $64`, Nummer in AX, Argumente ab 4(SP); das Kind bekommt auf 386 alle
  Register des Elternprozesses (forkret: POPAL der kopierten Ureg), AX = 0, also ESI/EDI
  statt R14/R15. Die Stack-Slots bleiben (nur SEG1–SEG4 frei pro Proc: ≤ 4 Slots à 64 MiB,
  ab 0x80000000 abwärts). libc-Crate: `usize_` = `c_ulong`, `jmp_buf` 6 Wörter auf 32 Bit.
- `objtype=386 tests/run.sh`: alle 12 ok im 9pc-Gast (qemu-system-i386, 4 CPUs), auf Anhieb;
  Ausgaben in `tests/out/$objtype`, Log `logs/guest-tests.386.log`.
- Stack: 9front-Programme auf 386 starten mit nur 4 Byte Ausrichtung (Kernel, Notes; der
  Baum `-mincoming-stack-boundary=2`). LLVM nimmt für i686 mit unbekanntem OS ohnehin 4 an
  (16 nur für Linux, Darwin, kFreeBSD, 64 Bit) und richtet Rahmen mit 16-Byte-Daten selbst
  aus (`andl $-16, %esp`, geprüft); das `S128` im Datenlayout ist nur ein Hinweis (ein
  anderes als LLVMs wäre für rustc ein Fehler).
- **cargo für 386** (`objtype=386 scripts/build-cargo.sh`, `objtype=386 tests/cargo.sh`: 5/5
  im 9pc-Gast: version, new/libgit2, search/curl+rustls, fetch/sqlite, refetch). Zwei
  C-Fehler, beide `-Wincompatible-pointer-types` (gcc 14+ ein Fehler) aus einer Ursache: gcc
  für i686-pc-plan9 hatte `size_t` = `unsigned long` (gccs Standard, wie Plan 9s `usize`),
  aber `uintptr_t` = `unsigned int` (glibc-stdint.h für 32 Bit). curls `rustls.c` nimmt
  `uintptr_t *` für rustls-ffis `size_t *` (wie bei amd64 vor APEs stdint-Fix). Behoben an
  der Wurzel: 9front-gcc `i386/plan9-386.h` INTPTR/UINTPTR_TYPE `long` (wie size_t und u.h's
  `uintptr`; gleiche Größe, keine ABI-Änderung), APEs `inttypes.h` PRI*PTR `l` dafür (nur
  unter `__plan9__`: der Baum selbst baut mit gcc -m32, dort bleibt int). libgit2 nahm bei
  `SIZE_MAX == UINT_MAX` `__builtin_uadd_overflow` (`unsigned int *`): jetzt die
  typgenerischen `__builtin_add/mul_overflow` (crate-Patch).
- **rustc nativ auf 386**: nativer gcc für 386 (`objtype=386 BUILD=… OUTDIR=$PWD/out-gcc
  PLAN9=../9front-gccwt ./build.sh hostlibs native plan9` in 9front-gcc, der Linker im Gast);
  LLVM `objtype=386 scripts/build-llvm.sh` (`build/llvm-plan9.386`, `cmake/plan9.cmake` nach
  `$objtype`), `objtype=386 scripts/llvm-config-plan9.sh`, `llvm-config` im
  `[target.i686-unknown-plan9]`; dann `./x build --stage 2 --host i686-unknown-plan9 --target
  i686-unknown-plan9 compiler/rustc` und `library` ebenso: statisch, 32 Bit, 522 MB mit
  Debug-Infos. Ohne Änderung am Compiler. `objtype=386 tests/rustc.sh` 3/3,
  `objtype=386 tests/cargo-build.sh` 10/10 (alle Stufen wie amd64, auch Proc-Macros als
  Programme, verschachtelt und abstürzend; serde-derive 2 min 18 s, 4 CPUs, 3 GB).
- Nach dem Neubau des stage-1-Compilers (neues Target) fehlte die stage-2-std für amd64: die
  ganze amd64-Kette (stage 2 rustc für plan9, library) neu bauen.
- `./x build --stage 2 library --host T --target T` entfernt die stage-2-std (und den stage-1-
  Sysroot) der anderen plan9-Targets: vor `tests/rustc.sh`/`cargo-build.sh` einer Architektur
  ihre stage-2-library bauen, danach `./x build --stage 1 library --target …` für alle (sonst
  E0463 bei `run.sh` und cargo).

## std-Testsuite im Gast (Teil 9, 2026-10-30)

`./x test library/std --target x86_64-unknown-plan9 --no-doc --no-fail-fast` mit
`runner = tests/std-runner.sh` im `[target.*-plan9]` von `bootstrap.toml`: cargo ruft ihn mit
jedem Testprogramm; er bootet je Programm einen Gast seiner Architektur (`file`: 386 oder
amd64), `/tmp` auf ext4, Loopback `127.1` und `::1`, 900 s Grenze, und gibt Ausgabe und Status
zurück (`STDTEST_LOG`: alle Gastprotokolle).
- Zwei Kopien von std im Unit-Testprogramm (die geprüfte und die des Sysroots über libtest):
  der Assembler sind jetzt naked functions (gemangelt, je Kopie eigen; `threadstart` über
  `sym`), die C-Exporte (`malloc` &c., `__plan9_*`) haben `no_mangle`/`export_name` nur außer
  `cfg(test)`, `PRIVTOP`/`MAIN_PID` eine Instanz (sgx' Muster: `export_name` +
  `linkage = "available_externally"` im Test). `current()` nimmt ThreadData nur mit der
  eigenen pid: die privaten Slots eines Procs sind eine Kopie der des Elternprocs (rfork).
- std-Korrekturen: `Instant` vorzeichenbehaftet (Nanosekunden, i64-Sekunden-Bereich wie
  timespec: `Instant - 10 Jahre` ging nicht); `OpenOptions` prüft wie unix (Meldungen,
  append+truncate); `set_times` setzt die mtime, atime still übergangen (stat(5): ein wstat
  ändert sie nicht; Variante „verträglich“, in `os::plan9::fs` beschrieben); `remove_dir_all`
  wie `rm -r` (ganz lesen, dann löschen: sonst rutschen Einträge nach, 32768 Dateien);
  `ExitStatus::code` wie APEs wait (Zahl, sonst 1, Note → keiner); NUL im Programmnamen
  `InvalidInput`; „malformed address“ `InvalidInput`; `read_buf` von Sockets ohne Vorab-
  Initialisierung; `Socket::as_raw`.
- ext4srv (gcc-Branch): atime beim Anlegen; wstat-Länge verlängert mit Nullen, kürzt mit
  genulltem Blockrest; `rwrite` liest die Länge frisch aus dem Inode (ein anderer fid änderte
  sie: das Auffüllen schrieb sonst über dessen Daten).
- Tests auf plan9 `ignore` mit Grund: Symlinks/Junctions (14 + 5), Hardlinks, Verzeichnis-
  Umbenennung über Verzeichnisse, atime setzen (3), TCP: halbes Schließen (`shutdown_smoke`,
  `close_read_wakes_up`), keepalive/ttl/hop_limit zurücklesen, linger, nonblocking, peek,
  Nagle; NUL in Umgebungswerten (rc-Listen). Testseitig: Pfadtrenner `/`, rwlock-Grenze wie
  futex, `$path` statt `PATH`, `env` = `rc -c 'cd /env; for(v in *) whatis $v'`, Shell
  `/bin/ape/sh`.
- **Ergebnisse** (2026-10-31): std amd64: Unit 343 ok, 39 ignoriert, 2 fehlgeschlagen
  (`multiple_connect_interleaved_*`, TCP unter Last), alle 29 Integrationsprogramme ok;
  std 386: Unit 345 ok, 38 ignoriert, 0 fehlgeschlagen (die beiden ext4srv-Tests erst mit
  dem neu gebauten 386-ext4srv), 29 ok. `./x test library/coretests library/alloctests`:
  amd64 und 386 ohne Fehler, ohne Anpassung (core 2970 bzw. 2969, 2 ignoriert wie upstream;
  alloc 339/1494/643 bzw. 338/1495/643, die beiden alloc_error-Programme).
- **Hänger der Tests unter Last: libgccs Unwinder ohne Sperren.** pstack zeigte einen Broken
  Proc in `__plan9_abort` ← `_Unwind_RaiseException_Phase2` (Panic in einem Test-Thread):
  gthr-posix prüft per schwachem Verweis auf `pthread_cancel`, ob Threads da sind; der zieht
  APEs pthread.o nicht ins Rust-Programm, die FDE-Listen blieben ungesperrt. Behoben in
  9front-gcc (`GTHREAD_USE_WEAK 0` in `gcc/config/plan9.h`). Das `abort` der Testkopie von std
  beendete nur deren Threads: main (Sysroot-std) wartete ewig.
- **Offen: TCP unter Last.** Mit 12 Test-Threads scheitern `multiple_connect_interleaved_*`
  (je 10 Verbindungen vor dem ersten accept): „connection refused“, „i/o on hungup channel“,
  danach liest die Gegenseite 0 Bytes (`buf[0] == 99`); auch einmal „listen hungup“ in
  `clone_accept_*`. Mit einem Test-Thread grün (40 Läufe). Nicht die incall-Grenze
  (`Maxincall` 10, keine Meldung „incall queue full“), keine doppelten Ports (zufällig aus
  2^15..2^16). Ungeklärt: 9fronts TCP über Loopback unter Last oder std's Netzcode.
- **Offen, Bug in 9front: TCP-`connect` über Loopback hängt unter Last** (nicht `/net/cs`, wie
  zuerst gedeutet: im Stack lag `call` von dial(2), das `connect` ans ctl schreibt). Mit C
  nachgestellt (`tools/csstress.c`, `tools/csstress.rc`), gleich mit dem kencc-Kernel: ein Fehler von
  9front, nicht vom gcc-Port (9front-gccwt MEMORY.md). std wählt numerische Adressen
  trotzdem direkt (`/net/tcp/clone`), cs ist für SocketAddr unnötig.
- **Offen: TCP halb schließen.** 9fronts ctl `close` legt auch die Leseseite auf (`tcpclose`:
  `qhangup(c->rq)`), `shutdown(Write)` beendet also beide Richtungen: eine Antwort der
  Gegenseite geht still verloren (HTTP-Clients, nc-artige Werkzeuge). Nur im Kernel lösbar.

## Tests

- `tests/run.sh`: baut `tests/*.rs` und `tests/c`, läuft im Gast, eine Zeile pro Test; Stand: alle 7 ok.
- `tests/nostd.rs` (core, libc von Hand), `tests/std1.rs` (args, scoped threads, Mutex unter
  Last, Condvar/Timeout, Barrier, RwLock, Kanäle rendezvous/gepuffert, TLS + Destruktor,
  sleep, SystemTime, HashMap, Float-Format), `tests/overflow.rs` (Überlauf in main und Thread
  → Broken): laufen im Gast (2026-10-29).

## async: mio und tokio (Teil 10, 2026-10-31)

Plan 9 hat kein epoll/kqueue/poll für native fds, nur blockierende Aufrufe.
- **std** (`sys/net/connection/plan9/pump.rs`): ein Socket, der nonblocking geschaltet oder
  gepeekt wird, bekommt eine Pumpe: ein Lese-Proc in einen Puffer (Strom 64 KiB, Datagramme,
  angenommene Rufe eines Listeners), ein Schreib-Proc aus einem Puffer (erst mit dem ersten
  write: ein nur lesender Socket kostet einen Proc). read/peek/recv/accept daraus oder
  WouldBlock, write hinein; eigener fd (dup), beim Schließen des letzten fds wird der Leser per
  Note (`rust: interrupt`) unterbrochen, der Schreiber schreibt aus. `shutdown(Write)` erst nach
  dem Gepufferten, über das beim Aufruf geöffnete ctl (ein späteres Öffnen per Name fand die
  Nummer schon an eine andere Verbindung vergeben). `os::plan9::net`: `readiness`,
  `set_ready_hook`, `connect_nonblocking` (ctl angelegt, data gleich offen, connect im Proc).
  Peek geht jetzt auch blockierend. `tests/nb.rs` (in run.sh).
- **mio** (`crates/mio-1.2.1`, `sys/plan9`): Selector = Liste geänderter fds + Condvar, die
  Hooks tragen ein; `select` liest `readiness`. Waker, connect/bind über std (kein Socket vor
  dem connect). `tests/crates/mioecho`.
- **tokio** (`crates/tokio-1.54.1`): ohne socket2 auf plan9 (keine BSD-Sockets): kein
  `TcpSocket`, linger/UDP-Optionen Unsupported, `peek_sender` über std's peek_from.
  `tests/crates/tokioecho`: multi-thread runtime, 20 Echo-Clients mit halbem Schließen, sleep,
  timeout, Kanal, fs. `tests/async.sh` (beide Architekturen).
- **Kernel** (9front-gccwt): TCP halb schließen (ctl `shutdown`), ein veralteter bypass-Zeiger
  bei Loopback legte neue Verbindungen auf (MEMORY.md dort).
- Kosten: ein bis zwei Procs pro nonblocking Verbindung (9fronts Grenze: conf.nproc).

## HTTP, HTTPS, DNS (Teil 11, 2026-10-31)

- **hyper-util** (`crates/hyper-util-0.1.21`): der `HttpConnector` (client-legacy, von reqwest
  benutzt) baut seinen Socket mit socket2 und tokios `TcpSocket`; auf plan9 verbindet er über
  `tokio::net::TcpStream::connect` (std's connect im Hintergrund). Lokale Adresse: Unsupported;
  keepalive wird nicht gesetzt (TCPs ctl `keepalive` hat std nicht).
- `tests/crates/httptest`: hyper-Server (HTTP/1.1 und HTTP/2, auto), hyper-Clients (http1,
  http2 mit 10 Strömen), hyper-utils Client (Pool), reqwest (GET, POST 1 MiB, JSON, chunked,
  404, timeout, 20 gleichzeitig, HTTP/2).
- `tests/crates/tlsdns`: DNS über `/net/cs` (std und tokio `lookup_host`, localhost, ein
  unbekannter Name, 10 gleichzeitig), HTTPS mit rustls (ring) gegen einen lokalen
  tokio-rustls-Server mit rcgen-Zertifikat (HTTP/1.1, HTTP/2 per ALPN, 1 MiB, unbekannter
  Aussteller abgelehnt) und zu crates.io mit den Wurzeln aus `/sys/lib/tls/ca.pem`. Braucht das
  Netz (QEMUs DHCP/DNS).
- 9front-Fehler gefunden: ndb/cs schnitt Fehlermeldungen auf 63 Zeichen ab (`curerr[64]` in
  `rwrite`), bei langen Namen fehlte „name does not exist“; im gcc-Branch behoben.
- `tests/async.sh` läuft alle vier (mio, tokio, http, tlsdns).
- Sporadisch „early eof“ in tokioecho (20 Clients auf einmal): 9fronts Kernel legte bei voller
  Anrufschlange (`Maxincall` 10) einen Anruf mit Reset auf, den der Client schon als
  verbunden hatte (sein connect war fertig): er las sofort EOF („eof after 0“, mit
  `Fsnewcall: incall queue full`). Im gcc-Branch behoben (9front-gccwt MEMORY.md): der Anruf
  bleibt im Limbo wie bei BSD/Linux mit vollem Backlog. tokioecho prüft es gezielt: ein
  std-Listener nimmt eine Sekunde lang nichts an, 30 Clients verbinden sich (alter Kernel: 20
  lesen EOF).

## tokio::process und tokio::signal (Teil 12, 2026-10-31)

- **std** `os::plan9::note`: `catch(name)` fängt eine Note (genau dieser Name; nicht `sys: …`,
  `alarm`, `rust: …`): das Programm stirbt nicht mehr daran, `Note::wait`/`take` zählen sie
  (Semaphor, `semrelease` im Note-Handler: kein Lock, keine Allokation). Eine Note an die
  Gruppe (Delete, `notepg`) erreicht jeden Proc des Programms; gezählt wird sie nur im Proc
  von `main` (`proc::main_pid`), die anderen machen weiter. Der Kernel lässt den Absender
  einer `notepg` aus (`postnotepg`: `p == up`): wer aus main an die eigene Gruppe postet,
  erreicht main nicht. Dazu fd-Traits für `ChildStdin/Stdout/Stderr` (wie unix).
- **Einschränkung**: eine gefangene Note unterbricht blockierte Systemaufrufe in allen Procs
  („interrupted“, `ErrorKind::Interrupted`). Unix startet sie meist neu (SA_RESTART), Plan 9
  nicht: std's eigene Stellen (Pumpe, Parken, wait, read_exact/read_to_end) und tokios
  Kind-Pipes wiederholen, ein einzelnes `read` eines Programms kann den Fehler sehen.
- **tokio::signal** (`signal/plan9.rs`): `ctrl_c()` = Note `interrupt`,
  `tokio::signal::plan9::note(name)`; ein Thread pro Note wartet am Semaphor und weckt einen
  watch-Kanal (wie tokios Registry unter unix; Noten vor dem Hinsehen fallen zusammen).
- **tokio::process** (`process/plan9.rs`, nach windows.rs): ein Waiter-Thread pro Kind forkt
  es selbst und wartet (std's `wait`): das Waitmsg geht an den forkenden Proc, der so lebt,
  bis das Kind endet (auch ein fallengelassenes Kind wird abgeholt). Pipes blockierend im
  Blocking-Pool (`Blocking`, Plan-9-Pipes sind nicht pollbar), Wiederholung bei
  „interrupted“. kill über `/proc/<pid>/ctl`. Kosten: ein Proc pro laufendem Kind.
- `tests/crates/tokioproc` in `tests/async.sh` (in eigener Note-Gruppe: `rfork s`, exec):
  output, Exit-Status, 1 MiB durch cat, kill, kill_on_drop, try_wait, 10 gleichzeitig, nicht
  gefunden, std-Zählung (einmal für die Gruppe), ctrl_c, eine Pipe über die Note hinweg,
  plan9::note.
- **Listener schließen synchron** (gefunden: `nb` „refused“ auf 386, seit Teil 10 rot): der
  Leser einer Pumpe blockiert in `listen` und hält die Ankündigung; `drop` gab nur die Note
  und kehrte zurück, ein gleich folgendes connect wurde noch angenommen und dann geschlossen
  (EOF statt „refused“). Jetzt wartet das Schließen auf das Ende des Lesers; die Note geht alle
  10 ms erneut (kommt sie, bevor er in `listen` ist, verpufft sie), höchstens 5 s. Der Port
  ist frei, wenn close zurückkehrt, wie unter unix.

## Installation und Werkzeuge (Teil 13, 2026-10-31)

- **Rebase** des gcc-Branchs auf front 350266104 (27 Commits von upstream, keine Konflikte;
  Manifeste neu, `news.c` weg): alle drei Architekturen bauen, run.sh und async.sh auf amd64
  und 386 grün.
- **rustdoc, rustfmt, clippy für 9front** (`./x build --stage 2 --host T --target T
  src/tools/rustdoc src/tools/rustfmt src/tools/clippy`): wie rustc statisch, ohne
  `rustc_driver`-dylib: `extern crate rustc_driver_impl as rustc_driver` (rustdoc, rustfmt's
  bin, clippy-driver), wo nur zum Linken gebraucht entfällt es (rustfmt's lib,
  clippy_utils); cargo-clippy bekommt `EXIT_FAILURE` aus einem kleinen Modul statt des ganzen
  Compilers. bootstrap legt für plan9 die rlibs der Compiler-Crates in den Sysroot (sonst nur
  rmeta: auf Linux sind sie in librustc_driver.so). rustdoc: `Os::Plan9` in `clean/cfg.rs`,
  sein eigenes `flock` nimmt `os::plan9::fs::lock_file` (DMEXCL). Crates: stacker 0.1.21 (ein
  Stack aus malloc wie wasm/hermit: std findet seine Thread-Daten über `_privates`, nicht über
  den Stackzeiger), dirs-sys 0.5.0 (`$home`, keine XDG-Verzeichnisse). Die Werkzeuge liegen in
  `build/x86_64-unknown-linux-gnu/stage2-tools/T/release` (stage2/bin legt bootstrap jedes Mal
  neu an). Gestrippt: rustdoc 165 MB, clippy-driver 169 MB, rustfmt 49 MB (amd64).
- **Installation** (`scripts/install.sh`): `out/$objtype/$objtype/rust` (bin, lib/rustlib/T/lib
  mit rlib und rmeta: bootstrap baut std mit `-Zembed-metadata=no`, ohne die rmeta findet
  rustc keine Metadaten; README) und zwei Archive zur optionalen Installation,
  Pfade ab `/`: `rust-T.tgz` (rustc, cargo, std, 117 MB) und `rust-tools-T.tgz` (150 MB).
  std aus dem stage-1-Sysroot (dieselben Dateien wie stage 2, das bootstrap hochhebt; ein
  stage-2-Build für ein Target entfernt die std der anderen). Auf die Install-CD nur auf
  Wunsch: `RUST_OUT` für 9front-gccwt's `stage-dist.sh` (`rustproto`).
- `tests/tools.sh`: entpackt die Archive mit 9fronts tar im Gast, dann rustc/cargo/rustdoc/
  rustfmt/clippy --version, cargo build/run, cargo fmt (--check vorher rot, nachher grün),
  cargo clippy (needless_return), cargo doc, cargo test mit Unit- und Doc-Test.

## arm64: aarch64-unknown-plan9 (Teil 14, 2026-10-31)

- **Toolchain**: 9front-gcc kann jetzt `aarch64-pc-plan9` (cross und nativ, `/arm64/gnu`; dort
  NOTES): char vorzeichenbehaftet, keine Outline-Atomics, crt*.o ohne Unwind-Tabellen. Im
  gcc-Branch: `arm64/plan9.ld` sammelt `.init_array`/`.gcc_except_table`, APEs `fenv.h` hat
  arm64s Werte.
- **Target** `aarch64_unknown_plan9.rs`: Datenlayout von aarch64-linux-gnu, `+v8a` ohne
  `+outline-atomics`, Linker `aarch64-pc-plan9-gcc`. `core::ffi::c_char` ist auf Plan 9 auf jeder
  Architektur `i8` (7c, jc; gccs DEFAULT_SIGNED_CHAR). libc: `jmp_buf` 22 Wörter.
- **std**: `proc.rs` runon/rforkthread/rforkrun in AArch64 (Nummer in R0, Argument bei 8(SP),
  `SVC #0`; das Kind bekommt alle Register, R0 = 0: privtop und Argument in x26/x27 wie APEs
  pthread.c). Die Stack-Slots passen in arm64s 32-GiB-Adressraum (EVASHIFT 36). Sonst nichts.
- **Gast**: QEMU virt unter TCG, ein Prozessor (`tests/guest.sh`: 9fronts qemu-Kernel startet
  keine weiteren, MEMORY.md des Trees); die ext4-Platte als virtio-blk-pci 1.0 (`/dev/sdG0`, sdF0
  ist u-boots FAT). Falle: der Gast bootet `arm64/9qemu.u`, das nur `build-boot.sh` erneuert —
  mit dem alten lief tokio ohne TCP-`shutdown` (EOF, unvollständige HTTP-Anfragen).
- **Ergebnisse**: run.sh 13/13, async.sh 5/5 (mio, tokio, http, tlsdns, proc), cargo.sh 5/5
  (cargo von Linux aus gebaut), rustc.sh 3/3 (rustc nativ: LLVM mit AArch64-Backend,
  `objtype=arm64 scripts/build-llvm.sh`, 15 min für rustc und die Werkzeuge).

## riscv64: riscv64gc-unknown-plan9 (Teil 15, 2026-11-10)

- **Toolchain**: 9front-gccs `riscv64-unknown-plan9` (rv64gc, lp64d, medany, `-mno-relax`, char
  vorzeichenbehaftet), gegen den Baum `../9front-riscv` (Branch riscv, auf gcc 82ddeb288
  rebased). Der alte Cross-gcc (Branch riscv, 21.10.) hatte noch die compat-Header im Präfix,
  die seitdem im APE des Baums liegen: `max_align_t`/`lldiv_t` doppelt. Neu gebaut vom Branch
  gpu (`objtype=riscv64 PLAN9=../9front-riscv ./build.sh cross`; der alte liegt in
  `build/riscv64.2026-10-21`); damit auch `GTHREAD_USE_WEAK 0` (gleichzeitige Panics).
- **Target** `riscv64gc_unknown_plan9.rs`: Datenlayout von riscv64gc-linux-gnu,
  `+m,+a,+f,+d,+c,+zicsr,+zifencei,-relax` (niemand setzt gp), `CodeModel::Medium`, lp64d,
  Linker `riscv64-unknown-plan9-gcc`. libc: `jmp_buf` 26 Wörter (sp, pc, s0-s11, fs0-fs11).
- **std**: `proc.rs` runon/rforkthread/rforkrun in RISC-V (Nummer und Ergebnis in s0,
  Argument bei 8(sp), `ecall`; das Kind bekommt die ganze Ureg, s0 = 0: privtop und Argument
  in s1/s2 wie APEs pthread.c). Stack-Slots passen (USTKTOP 256 GiB, Sv39).
- `bootstrap.toml`: die Cross-gccs liegen jetzt in `9front-gcc/build/` (nicht mehr
  `build-gcc/`); `tests/run.sh` braucht dafür `GCC9BUILD=…/9front-gcc/build`.
- **cc-Crate** (1.4.3 cargo, 1.10.0 async-Tests, 1.11.1 GUI-Tests; `patches/crates/cc-*.patch`):
  für riscv64 gibt es `-march=rv64gc -mabi=lp64d` nur unter Linux, FreeBSD, NetBSD (…), sonst
  `-mabi=lp64` (Soft-Float): ring's C-Objekte linkten nicht („can't link soft-float modules with
  double-float modules“). `plan9` in die Liste.
- Falle: nach dem Neubau des stage-1-rustc (gleiche Versionszeile) nimmt cargo die alten
  Host-Artefakte (Build-Skripte) weiter: E0460/E0463. `target/release` der Testcrates und
  `tests/out/cargo-port/release` löschen.
- **Ergebnis** (QEMU virt, TCG, 4 Harts, `GCC9BUILD=…/9front-gcc/build`): run.sh 13/13,
  async.sh 5/5 (mio, tokio, http, tlsdns, proc), cargo.sh 5/5 (cargo von Linux aus gebaut,
  `objtype=riscv64 scripts/build-cargo.sh`). Nicht gelaufen: std-Testsuite; kein rustc nativ
  (LLVM für riscv64).

## Offen

Stand 2026-10-31: amd64 und 386 cross und nativ (std, rustc, cargo, Proc-Macros als Programme,
Datei-Locks), std/core/alloc-Testsuiten im Gast grün bis auf die TCP-Tests unter Last.
- **async**: TCP-keepalive in std fehlt; eine gefangene Note unterbricht blockierte Aufrufe
  (Teil 12).
- **9front-Kernel** (nicht vom Port, 9front-gccwt MEMORY.md): TCP-`connect` über Loopback hängt
  unter Last (`tools/csstress`, auch mit kencc-Kernel).
- **arm64** (Teil 14): std-Testsuite nicht gelaufen; der Gast hat nur einen Prozessor.
- **Werkzeuge**: die Doc-Tests der std nie gelaufen (`--no-doc`); rustdoc und clippy-driver
  enthalten je den ganzen Compiler (zusammen mit rustc ~490 MB; ein Programm für alle drei
  wäre ~170 MB).
- APEs Kopier-Procs: nach `close` mit laufendem read bleibt ihr Puffer stehen (Leck, cargo).
- acid liest die ELF-Symbole des gcc-Ports nicht (tools/pstack + addr2line statt dessen).
- Was Plan 9 nicht hat (bleibt so, Tests `ignore` mit Grund): Symlinks, Hardlinks, atime
  setzen, TTL/keepalive zurücklesen, linger.

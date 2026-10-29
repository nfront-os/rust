#!/bin/bash
# Build build/ape9: APE (libap, libbsd of the 9front tree; libapecompat of 9front-gcc, without
# its pthreads) with every symbol it defines renamed __ape_<name>, for C libraries in Rust
# programs (ape9/ape9init.c). Outputs: libape9-{ap,bsd,compat,init}.a, ape9.syms (the
# renames, also for scripts/ape9-ar), link.args (what rustc passes the linker).
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
GCC9=${GCC9:-$TOP/../9front-gcc}
PLAN9=${PLAN9:-$TOP/../9front}
X=$GCC9/build/amd64/cross/bin/x86_64-pc-plan9
O=$TOP/build/ape9
rm -rf "$O"; mkdir -p "$O"
cp "$PLAN9/amd64/lib.gcc/ape/libap.a" "$O/libape9-ap.a"
cp "$PLAN9/amd64/lib.gcc/ape/libbsd.a" "$O/libape9-bsd.a"
cp "$GCC9/build/amd64/cross/x86_64-pc-plan9/lib/libapecompat.a" "$O/libape9-compat.a"
# libc's malloc instead; the toolchain's pthreads define __plan9_mainstack, as std does
$X-ar d "$O/libape9-ap.a" malloc.o
$X-ar d "$O/libape9-compat.a" pthread.o
# every symbol defined, but the allocator's (libc's)
for a in ap bsd compat; do
	$X-nm -g --defined-only "$O/libape9-$a.a" 2>/dev/null | awk 'NF==3 {print $3}'
done | sort -u | grep -vxE 'malloc|free|realloc' | awk '{print $1, "__ape_" $1}' > "$O/ape9.syms"
for a in ap bsd compat; do
	$X-objcopy --redefine-syms="$O/ape9.syms" "$O/libape9-$a.a"
done
# _envsetup takes no notes: the program has its handler
mkdir -p "$O/tmp"; (cd "$O/tmp" && $X-ar x "$O/libape9-ap.a" _envsetup.o)
$X-objcopy --redefine-sym __ape__NOTIFY=__ape9_nonotify "$O/tmp/_envsetup.o"
$X-ar r "$O/libape9-ap.a" "$O/tmp/_envsetup.o"; rm -rf "$O/tmp"
$X-gcc -O2 -c -o "$O/ape9init.o" "$TOP/ape9/ape9init.c"
$X-ar rcs "$O/libape9-init.a" "$O/ape9init.o"
for a in ap bsd compat; do $X-ranlib "$O/libape9-$a.a"; done
echo "-L$O -Wl,-u,__ape9_init -Wl,--start-group -lape9-init -lape9-compat -lape9-bsd -lape9-ap -Wl,--end-group" > "$O/link.args"
wc -l < "$O/ape9.syms"

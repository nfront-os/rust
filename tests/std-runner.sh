#!/bin/bash
# The runner of bootstrap's tests for the plan9 targets (x86_64, i686, aarch64, riscv64gc)
# ([target.*] runner in bootstrap.toml: cargo's CARGO_TARGET_*_RUNNER): runs the test
# binary (cargo's first argument, the rest are its arguments) in a 9front guest of its
# objtype (tests/guest.sh, one guest per binary), /tmp on an ext4 disk, loopback up for the
# net tests (127.1 and ::1). Its output is this script's, its status too (a failure: 101, as the harness).
# usage: tests/std-runner.sh binary [args...]
# env: MEM (guest MB, 2048), SMP (4), STDTEST_TIMEOUT (seconds per binary, 900; riscv64 3600),
#      PLAN9 (the system booted; default ../9front-gccwt), STDTEST_LOG (all guest logs, appended)
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
bin=$1
shift
case $(file -b "$bin") in
*i386*|*80386*)	export objtype=386 ;;
*aarch64*)	export objtype=arm64 ;;
*RISC-V*)	export objtype=riscv64 ;;
*)	export objtype=amd64 ;;
esac
# riscv64: its own tree (branch riscv); under TCG its std unit tests take ~2600 s (the fs
# tests that make thousands of files)
[ $objtype != riscv64 ] || export PLAN9=${PLAN9_RISCV:-$TREE_RISCV}
[ $objtype != riscv64 ] || STDTEST_TIMEOUT=${STDTEST_TIMEOUT:-3600}
export PLAN9=${PLAN9:-$TREE}
name=$(basename "$bin")
# rc quoting: each argument in '', a ' doubled
args=
for a in "$@"; do
	args="$args '${a//\'/\'\'}'"
done
empty=$(mktemp -d)
trap 'rm -rf "$empty"' EXIT
log=$(MEM=${MEM:-2048} CMDWAIT=${STDTEST_TIMEOUT:-900} GUEST_TIMEOUT=$(( ${STDTEST_TIMEOUT:-900} + 300 )) \
	"$TOP/tests/guest.sh" -D "/guest/empty=$empty" -f "$bin" -- \
	"ip/ipconfig loopback /dev/null 127.1; ip/ipconfig loopback /dev/null ::1 /128" \
	"cd /tmp; RUST_BACKTRACE=1 /tests/$name$args >[2=1]; echo STDTEST-STATUS \$status" 2>&1)
# a guest that hit the time limit gives no log (boot-cli prints it at its end): its
# console has what ran (the tests' results so far)
if ! echo "$log" | grep -q '^STDTEST-STATUS'; then
	log=$(tr -d '\r' < "${CLI_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/nfront/cli}/console.log")
	# a line as the command's for the awk below, then what came after the command
	log=$(echo "/tests/$name STDTEST-STATUS"; echo "$log" | sed -n '/STDTEST-STATUS/,$p' | sed 1d)
fi
[ -z "$STDTEST_LOG" ] || { echo "=== $name$args"; echo "$log"; } >> "$STDTEST_LOG"
# the program's output: after its command line, up to the status
echo "$log" | awk -v n="/tests/$name" '
	index($0, n) && index($0, "STDTEST-STATUS") { on = 1; next }
	/^STDTEST-STATUS/ { st = substr($0, 16); done = 1; exit }
	on { print }
	END { if(!done) { print "std-runner: no status from the guest (hung or crashed?)"; exit 101 }
	      exit (st == "" ? 0 : 101) }'

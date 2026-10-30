#!/bin/bash
# Build the test programs (tests/*.rs) with the stage-1 rustc for x86_64-unknown-plan9 and
# run them in the 9front guest (../9front's scripts/guest/boot-cli.sh). One line per
# test: "name ok" or "name FAIL ...". Log: logs/guest-tests.log (386: guest-tests.386.log).
# objtype=386: i686-unknown-plan9 in a 9pc guest (the 9front tree built for 386: PLAN9).
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
GCC9=${GCC9:-$TOP/../9front-gcc}
# its build directory with the toolchain over gcc's APE (9front branch gcc)
GCC9BUILD=${GCC9BUILD:-$GCC9/build-gcc}
case ${objtype:=amd64} in
amd64)	T=x86_64-unknown-plan9 X=x86_64-pc-plan9 LOG=guest-tests.log ;;
386)	T=i686-unknown-plan9 X=i686-pc-plan9 LOG=guest-tests.386.log
	export PLAN9=${PLAN9:-$TOP/../9front-gccwt} ;;
*)	echo "run.sh: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
export PATH=$GCC9BUILD/$objtype/cross/bin:$PATH
R=$TOP/rust/build/host/stage1/bin/rustc
O=$TOP/tests/out/$objtype
mkdir -p "$O" "$TOP/logs"
cd "$TOP/tests"
$R --edition 2024 --target $T -O -C panic=abort nostd.rs -o "$O/nostd"
for t in std1 std2 unwind overflow lock; do
	$R --edition 2024 --target $T -O -g $t.rs -o "$O/$t"
done
# C and C++ against APE in Rust programs, compiled as any APE program
$X-gcc -O2 -c -o "$O/apehello.o" c/apehello.c
$X-ar rcs "$O/libapehello.a" "$O/apehello.o"
$R --edition 2024 --target $T -O -g c/main.rs -L "$O" -l static=apehello -o "$O/cmain"
$X-gcc -O2 -c -o "$O/cexit.o" c/cexit.c
$X-ar rcs "$O/libcexit.a" "$O/cexit.o"
$R --edition 2024 --target $T -O -g c/cexit.rs -L "$O" -l static=cexit -o "$O/cexit"
$X-g++ -O2 -c -o "$O/cxx.o" c/cxx.cc
$X-ar rcs "$O/libcxx.a" "$O/cxx.o"
$R --edition 2024 --target $T -O -g c/cxxmain.rs -L "$O" -l static=cxx -l stdc++ -o "$O/cxxmain"
# rc: run a test, its status the result
run() { echo "$2; if(~ \$status '') echo $1 ok; if not echo $1 FAIL \$status"; }
cmds=(
	"ip/ipconfig loopback /dev/null 127.1"
	"$(run nostd /tests/nostd)"
	"$(run std1 "/tests/std1 'a b' ü")"
	"$(run std2 /tests/std2)"
	"$(run unwind /tests/unwind)"
	# a stack overflow faults: the proc is Broken
	"/tests/overflow main >[2]/dev/null & sleep 2; if(ps | grep -s 'Broken +overflow') echo overflow ok; if not echo overflow FAIL"
	"$(run cmain /tests/cmain)"
	"$(run cxxmain /tests/cxxmain)"
	# file locks: ramfs keeps DMEXCL; /tmp as its server has it
	"ramfs -m /n/ram; $(run lock '/tests/lock /n/ram')"
	"$(run lock-any '/tests/lock /tmp any')"
	# C's exit and abort end the whole program (its sleeping thread too), the status for rc
	"/tests/cexit 0; if(~ \$status *3) echo cexit-exit ok; if not echo cexit-exit FAIL \$status"
	# abort: the other procs go, the aborting one stays Broken (for acid), then slay
	"/tests/cexit 1 >[2]/dev/null & sleep 2; if(~ \`{ps | grep cexit | wc -l} 1 && ps | grep -s 'Broken +cexit') echo cexit-abort ok; if not echo cexit-abort FAIL; slay cexit | rc"
	"sleep 1; if(ps | grep -s cexit) echo cexit-procs FAIL; if not echo cexit-procs ok"
)
"$TOP/tests/guest.sh" -f "$O/nostd" "$O/std1" "$O/std2" "$O/unwind" "$O/overflow" "$O/cmain" "$O/cxxmain" "$O/cexit" "$O/lock" -- "${cmds[@]}" > "$TOP/logs/$LOG" 2>&1
tests="nostd std1 std2 unwind overflow cmain cxxmain lock lock-any cexit-exit cexit-abort cexit-procs"
grep -E "^(${tests// /|})(: FAIL| (ok|FAIL))" "$TOP/logs/$LOG" || { tail -20 "$TOP/logs/$LOG"; exit 1; }
[ "$(grep -cE "^(${tests// /|}) ok" "$TOP/logs/$LOG")" = "$(echo $tests | wc -w)" ]

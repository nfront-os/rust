#!/bin/bash
# Build the test programs (tests/*.rs) with the stage-1 rustc for x86_64-unknown-plan9 and
# run them in the 9front guest (../9front's scripts/guest/boot-cli.sh). One line per
# test: "name ok" or "name FAIL ...". Log: logs/guest-tests.log.
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
GCC9=${GCC9:-$TOP/../9front-gcc}
export PATH=$GCC9/build/amd64/cross/bin:$PATH
R=$TOP/rust/build/host/stage1/bin/rustc
O=$TOP/tests/out
mkdir -p "$O" "$TOP/logs"
cd "$TOP/tests"
$R --edition 2024 --target x86_64-unknown-plan9 -O -C panic=abort nostd.rs -o "$O/nostd"
for t in std1 std2 unwind overflow; do
	$R --edition 2024 --target x86_64-unknown-plan9 -O -g $t.rs -o "$O/$t"
done
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
)
"$TOP/tests/guest.sh" -f "$O/nostd" "$O/std1" "$O/std2" "$O/unwind" "$O/overflow" -- "${cmds[@]}" > "$TOP/logs/guest-tests.log" 2>&1
tests="nostd std1 std2 unwind overflow"
grep -E "^(${tests// /|})(: FAIL| (ok|FAIL))" "$TOP/logs/guest-tests.log" || { tail -20 "$TOP/logs/guest-tests.log"; exit 1; }
[ "$(grep -cE "^(${tests// /|}) ok" "$TOP/logs/guest-tests.log")" = "$(echo $tests | wc -w)" ]

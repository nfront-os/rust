#!/bin/bash
# Run programs in the 9front guest (../9front's scripts/guest/boot-cli.sh): the files
# given are copied to /tests; each further argument is one rc command line.
# usage: tests/guest.sh [-x /guest/path=hostfile]... [-d /guest/dir=hostdir]... -f file... -- cmd...
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
PLAN9=$(cd "${PLAN9:-$TOP/../9front}" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/extra/tests"
while [ $# -gt 0 ] && [ "$1" != -- ]; do
	case $1 in
	-f) ;;
	-x) shift; dst=${1%%=*}; mkdir -p "$T/extra$(dirname "$dst")"; cp "${1#*=}" "$T/extra$dst" ;;
	-d) shift; dst=${1%%=*}; mkdir -p "$T/extra$dst"; cp -r "${1#*=}"/. "$T/extra$dst/" ;;
	*) cp "$1" "$T/extra/tests/" ;;
	esac
	shift
done
[ "$1" = -- ] && shift
cd "$PLAN9"
UFS_EXTRA=$T/extra timeout ${GUEST_TIMEOUT:-600} bash scripts/guest/boot-cli.sh "$@" > "$T/log" 2>&1 || true
tr -d '\r' < "$T/log" | sed -n '/^init: starting/,$p'

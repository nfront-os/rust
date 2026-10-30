#!/bin/bash
# tools/pstack-resolve.sh binary [log]: tools/pstack's lines (from a guest log) with the
# functions addr2line finds for them in the unstripped binary, one proc after another
# (one addr2line for all: it reads the debug information once).
B=${1:?usage: pstack-resolve.sh binary [log]}
T=$(mktemp)
trap 'rm -f "$T"' EXIT
tr -d '\r' < "${2:-/dev/stdin}" | grep -E '^pstack [0-9]+ (pc|ret) ' > "$T"
awk '{print $4}' "$T" | addr2line -f -C -e "$B" | awk 'NR % 2 == 1' | cut -c1-110 |
	paste -d' ' "$T" - | while read -r _ pid kind addr f; do
	[ "$kind" = pc ] && echo "== $pid"
	[ "$f" = "??" ] || echo "  $kind $addr $f"
done

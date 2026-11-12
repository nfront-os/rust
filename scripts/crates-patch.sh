#!/bin/bash
# The crates.io crates changed for 9front: crates/<name>-<version> (scripts/crate.sh) is the
# registry's copy with patches/crates/<name>-<version>.patch applied.
#   scripts/crates-patch.sh diff    write the patches from crates/ (after changing it)
#   scripts/crates-patch.sh apply   make crates/ from the published crates and the patches
#                                   (all of them anew: changes in crates/ are lost)
#   scripts/crates-patch.sh init    the same for the crates not in crates/ yet (fetch.sh)
# The published crate: cargo's registry if it has it, else the .crate from crates.io
# into dl/crates/, checked against the sha256 of crates.io's index.
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
P=$TOP/patches/crates
REG=$(ls -d ~/.cargo/registry/src/*/ 2>/dev/null | head -1)
# the published sources of crate $1 (name-version): their directory
published(){
	local c=$1 name ver d=$TOP/dl/crates
	[ -n "$REG" ] && [ -d "$REG/$c" ] && { echo "$REG/$c"; return; }
	[ -d "$d/$c" ] && { echo "$d/$c"; return; }
	name=${c%%-[0-9]*} ver=${c#"${c%%-[0-9]*}"-}
	mkdir -p "$d"
	curl -fsSL -o "$d/$c.crate" "https://static.crates.io/crates/$name/$c.crate" ||
		{ echo "crates-patch: cannot fetch $c" >&2; return 1; }
	sum=$(python3 -I - "$name" "$ver" <<'PY'
import json, sys, urllib.request
n, v = sys.argv[1], sys.argv[2]; l = n.lower()
pre = {1: '1', 2: '2', 3: '3/' + l[0]}.get(len(l), l[:2] + '/' + l[2:4])
for line in urllib.request.urlopen('https://index.crates.io/%s/%s' % (pre, l)):
    e = json.loads(line)
    if e['vers'] == v: print(e['cksum'])
PY
)
	echo "$sum  $d/$c.crate" | sha256sum -c --quiet ||
		{ echo "crates-patch: $c.crate: not the sha256 of crates.io's index" >&2; rm -f "$d/$c.crate"; return 1; }
	tar -xzf "$d/$c.crate" -C "$d"
	echo "$d/$c"
}
case $1 in
diff)
	mkdir -p "$P"
	for d in "$TOP"/crates/*/; do
		c=$(basename "$d")
		e=$(printf '%s' "$c" | sed 's/[.+]/\\&/g')	# the name in a regular expression
		pub=$(published "$c")
		(cd "$(dirname "$pub")" && diff -ruN --exclude=target "$c" "$d" || true) |
			sed -E "s|^diff -ruN '--exclude=target' $e/([^ ]*) $TOP/crates/$e/.*|diff -ruN a/\\1 b/\\1|;
				s|^--- $e/([^\t]*).*|--- a/\\1|; s|^\\+\\+\\+ $TOP/crates/$e/([^\t]*).*|+++ b/\\1|" > "$P/$c.patch"
			# (no local paths, no file times: the same crates give the same patches)
		[ -s "$P/$c.patch" ] || rm "$P/$c.patch"
	done
	ls "$P"
	# native libc functions declared by hand: without __p9_ they are APE's (README.md)
	nat=$(nm -g --defined-only "${PLAN9:-$TREE_GCC}/amd64/lib.gcc/libp9.a" 2>/dev/null |
		awk 'NF == 3 && $3 ~ /^__p9_/ {print substr($3, 6)}' | sort -u)
	for p in "$P"/*.patch; do
		awk '/^\+.*extern "C"/ {ext = 1}
			/^\+}/ {ext = 0}
			/^\+.*link_name *= *"__p9_/ {skip = 1; next}
			ext && /^\+ *(pub )?fn [A-Za-z_0-9]+\(/ {
				match($0, /fn [A-Za-z_0-9]+/); n = substr($0, RSTART + 3, RLENGTH - 3)
				if(!skip) print FILENAME ": " n }
			{skip = 0}' "$p"
	done | while read -r f n; do
		echo "$nat" | grep -qx "$n" && echo "warning: ${f%:} declares $n: APE's, not the native libc's (link_name = \"__p9_$n\")" >&2 || true
	done
	;;
apply|init)
	mkdir -p "$TOP/crates"
	for p in "$P"/*.patch; do
		c=$(basename "$p" .patch)
		[ "$1" = init ] && [ -d "$TOP/crates/$c" ] && continue
		pub=$(published "$c")
		rm -rf "$TOP/crates/$c"
		cp -r "$pub" "$TOP/crates/$c"
		patch -s -p1 -d "$TOP/crates/$c" < "$p"
	done
	;;
*)
	echo "usage: $0 diff|apply|init" >&2; exit 1 ;;
esac

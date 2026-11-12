#!/bin/bash
# The crates.io crates changed for 9front: crates/<name>-<version> (scripts/crate.sh) is the
# registry's copy with patches/crates/<name>-<version>.patch applied.
#   scripts/crates-patch.sh diff    write the patches from crates/ (after changing it)
#   scripts/crates-patch.sh apply   make crates/ from the registry and the patches
#                                   (cargo fetch first: the registry needs the sources)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# TREE, TREE_GCC, GCC_DIR, ... (the distribution's paths.env)
P=$TOP/patches/crates
REG=$(ls -d ~/.cargo/registry/src/*/ | head -1)
case $1 in
diff)
	mkdir -p "$P"
	for d in "$TOP"/crates/*/; do
		c=$(basename "$d")
		(cd "$REG" && diff -ruN --exclude=target "$c" "$d" || true) |
			sed "s|^--- $c/|--- a/|; s|^+++ $TOP/crates/$c/|+++ b/|" > "$P/$c.patch"
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
apply)
	for p in "$P"/*.patch; do
		c=$(basename "$p" .patch)
		rm -rf "$TOP/crates/$c"
		cp -r "$REG/$c" "$TOP/crates/$c"
		patch -s -p1 -d "$TOP/crates/$c" < "$p"
	done
	;;
*)
	echo "usage: $0 diff|apply" >&2; exit 1 ;;
esac

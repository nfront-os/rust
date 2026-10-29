#!/bin/bash
# The crates.io crates changed for 9front: crates/<name>-<version> (scripts/crate.sh) is the
# registry's copy with patches/crates/<name>-<version>.patch applied.
#   scripts/crates-patch.sh diff    write the patches from crates/ (after changing it)
#   scripts/crates-patch.sh apply   make crates/ from the registry and the patches
#                                   (cargo fetch first: the registry needs the sources)
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
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

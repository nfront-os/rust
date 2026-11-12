#!/bin/bash
# Copy a crate from cargo's registry into crates/<name>-<version> for its 9front changes,
# kept in this repository (the first commit of each is the crate as published), and list it
# in cargo-plan9.toml.in ([patch.crates-io]; cargo --config cargo-plan9.toml, written from it).
# usage: scripts/crate.sh name version
set -e
TOP=$(cd -P "$(dirname "$0")/.." && pwd -P)
name=$1 ver=$2
src=$(ls -d ~/.cargo/registry/src/*/"$name-$ver" | head -1)
[ -d "$src" ] || { echo "$name-$ver not in the registry" >&2; exit 1; }
dst=$TOP/crates/$name-$ver
[ -d "$dst" ] || cp -r "$src" "$dst"
# several versions of a crate: patch entries need a key of their own
key=$name
grep -q "^$name = " "$TOP/cargo-plan9.toml.in" 2>/dev/null && key="$name-${ver//./_}"
grep -q "crates/$name-$ver\"" "$TOP/cargo-plan9.toml.in" 2>/dev/null ||
	echo "$key = { package = \"$name\", path = \"@TOP@/crates/$name-$ver\" }" >> "$TOP/cargo-plan9.toml.in"
"$TOP/scripts/cargo-config.sh"
echo "$dst"

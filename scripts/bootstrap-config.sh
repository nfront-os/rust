#!/bin/bash
# rust/bootstrap.toml (Rust's ./x configuration) from bootstrap.toml.example: $GCC9 becomes
# nfront's gcc (GCC_DIR of dist/paths.env), $RUST9 this repository; absolute paths, only in
# the written file (rust/ is the archive, not in this repository).
# usage: scripts/bootstrap-config.sh   (fetch.sh runs it)
set -e
TOP=$(cd -P "$(dirname "$0")/.." && pwd -P)
. "$TOP/scripts/nfront-paths.sh"	# GCC_DIR
GCC9=$(cd -P "${GCC9:-$GCC_DIR}" 2>/dev/null && pwd -P) || GCC9=${GCC9:-$GCC_DIR}
sed "s#\$GCC9#$GCC9#g; s#\$RUST9#$TOP#g" "$TOP/bootstrap.toml.example" > "$TOP/rust/bootstrap.toml.new"
mv "$TOP/rust/bootstrap.toml.new" "$TOP/rust/bootstrap.toml"

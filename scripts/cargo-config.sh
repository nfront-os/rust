#!/bin/bash
# cargo-plan9.toml from cargo-plan9.toml.in: @TOP@ becomes this repository's path (cargo
# takes a relative path in a --config file from the directory above the file's, so the
# paths are absolute, and only in the written file, which is not in git)
# usage: scripts/cargo-config.sh   (fetch.sh and crate.sh run it)
set -e
TOP=$(cd -P "$(dirname "$0")/.." && pwd -P)
sed "s#@TOP@#$TOP#g" "$TOP/cargo-plan9.toml.in" > "$TOP/cargo-plan9.toml"

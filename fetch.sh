#!/bin/bash
# The sources from archives instead of clones: each line of sources.txt is
# downloaded to dl/, checked (signature url.asc against keys/<key>.pub where a key
# is named; sha256.txt always), unpacked to <name>/ and made a repository of its own,
# the archive as a commit with a tag (rust: nightly-<date>, from the URL; libc: its
# version; rust's git data is rust.git beside rust/).  scripts/rust-patches.sh apply
# then puts the port on the branches plan9; then rust/bootstrap.toml, crates/ and
# cargo-plan9.toml are written (scripts/bootstrap-config.sh, crates-patch.sh init,
# cargo-config.sh): after fetch.sh the sources are complete for ./x and build-cargo.sh.
# A <name>/ that is there is left as it is.
# usage: ./fetch.sh
set -e
cd "$(dirname "$0")"
mkdir -p dl/sig
while IFS=$'\t' read -r name url file key; do
	case $name in ''|\#*) continue ;; esac
	f=dl/$file
	[ -s "$f" ] || curl -fL -o "$f" "$url"
	if [ "$key" != - ]; then
		sig=dl/sig/$file.asc
		[ -s "$sig" ] || curl -fL -o "$sig" "$url.asc"
		gpg --dearmor < "keys/$key.pub" > "dl/sig/$key.gpg"
		gpgv --keyring "$PWD/dl/sig/$key.gpg" "$sig" "$f" 2>/dev/null ||
			{ echo "fetch: bad signature on $f" >&2; exit 1; }
	fi
	grep " $f\$" sha256.txt | sha256sum -c --quiet ||
		{ echo "fetch: $f: not the sha256 of sha256.txt" >&2; exit 1; }
	[ -d "$name" ] && { echo "fetch: $name/ is there, left as it is"; continue; }
	case $name in
	rust)	tag=nightly-$(echo "$url" | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}') ;;
	*)	tag=$(echo "$file" | sed -E "s/^$name-//; s/\.(crate|tar\.[a-z]+)\$//") ;;
	esac
	rm -rf "$name.tmp"
	mkdir "$name.tmp"
	tar -xf "$f" -C "$name.tmp" --strip-components=1
	(cd "$name.tmp" && git init -q -b archive && git add -A &&
	 git -c user.name=archive -c user.email=archive@localhost commit -qm "$name $tag (the archive)" &&
	 git tag "$tag")
	mv "$name.tmp" "$name"
	if [ "$name" = rust ]; then
		# the git data beside the tree: Rust's bootstrap takes rust/ for an archive (and
		# its LLVM by git-commit-info) only without rust/.git
		mv rust/.git rust.git
		git --git-dir=rust.git config core.worktree ../rust
	fi
	echo "fetch: $name/: $tag"
done < sources.txt
scripts/rust-patches.sh apply
scripts/bootstrap-config.sh	# rust/bootstrap.toml (Rust's ./x configuration)
scripts/crates-patch.sh init	# crates/: the crates.io crates with our patches (those not there yet)
scripts/cargo-config.sh	# cargo-plan9.toml (the patched crates of crates/)

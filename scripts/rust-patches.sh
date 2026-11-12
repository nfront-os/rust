#!/bin/bash
# The port's changes as patch series against the archives fetch.sh unpacks (rust/:
# the nightly, a repository with the archive tagged nightly-<date>; libc/: the crate,
# tagged with its version), one series per part, as git format-patch writes them:
#   patches/<name>/BASE            "<upstream url> <commit or version>" (for reference)
#   patches/<name>/NNNN-*.patch    the commits, in order
# rust's parts cargo, backtrace and llvm-project are directories of rust/ (the archive
# has them): their series apply there (git am --directory) and are written from
# there (--relative); rust's own series is rust/ without those directories.
#   scripts/rust-patches.sh diff   write the series from the branches plan9 (after a commit)
#   scripts/rust-patches.sh apply  make the branches plan9: the archive's tag and the
#                                  series; stops when plan9 has commits the series lack
#                                  (run diff first; FORCE=1 overrides), keeps the old
#                                  plan9 as plan9-before-apply
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
P=$TOP/patches
# name, repository, directory in it ("." for all of it but the others), upstream url
parts=(
	"rust rust . https://github.com/rust-lang/rust.git"
	"cargo rust src/tools/cargo https://github.com/rust-lang/cargo.git"
	"backtrace rust library/backtrace https://github.com/rust-lang/backtrace-rs.git"
	"llvm-project rust src/llvm-project https://github.com/rust-lang/llvm-project.git"
	"libc libc . https://github.com/rust-lang/libc.git"
)
SUBDIRS=(src/tools/cargo library/backtrace src/llvm-project)
die(){ echo "rust-patches: $*" >&2; exit 1; }
# the archive's tag of a repository (the one tag there is)
tag(){ git -C "$TOP/$1" describe --tags --abbrev=0 archive; }
# the pathspec of a part
spec(){
	if [ "$2" != . ]; then echo "$2"
	elif [ "$1" = rust ]; then echo .; for d in "${SUBDIRS[@]}"; do echo ":(exclude)$d"; done
	else echo .; fi
}
ids(){ git -C "$1" patch-id --stable | awk '{print $1}' | sort; }

case $1 in
diff)
	for r in "${parts[@]}"; do
		set -- $r
		name=$1 repo=$2 dir=$3 url=$4
		d=$TOP/$repo
		t=$(tag "$repo")
		rel=(); [ "$dir" = . ] || rel=(--relative="$dir")
		old=$(cut -d' ' -f2 "$P/$name/BASE" 2>/dev/null)
		rm -rf "$P/$name"
		mkdir -p "$P/$name"
		echo "$url ${old:-$t}" > "$P/$name/BASE"
		mapfile -t ps < <(spec "$name" "$dir")
		git -C "$d" format-patch -q --no-signature --zero-commit "${rel[@]}" -o "$P/$name" "$t..plan9" -- "${ps[@]}"
		echo "$name: $(ls "$P/$name" | grep -c '\.patch$') patches on $t"
	done ;;
apply)
	for repo in rust libc; do
		d=$TOP/$repo
		[ -d "$d/.git" ] || die "$repo: no $repo/ (./fetch.sh)"
		t=$(tag "$repo")
		if git -C "$d" rev-parse -q --verify plan9 >/dev/null; then
			if [ "$FORCE" != 1 ]; then
				want=$(for r in "${parts[@]}"; do set -- $r; [ "$2" = "$repo" ] && cat "$P/$1"/*.patch; done | ids "$d")
				have=$(git -C "$d" log -p --reverse "$t..plan9" | ids "$d")
				[ "$have" = "$want" ] || [ -z "$have" ] ||
					die "$repo: plan9 differs from the patches (run diff first, or FORCE=1)"
			fi
			git -C "$d" branch -f plan9-before-apply plan9
		fi
		git -C "$d" checkout -q -B plan9 "$t"
		for r in "${parts[@]}"; do
			set -- $r
			[ "$2" = "$repo" ] || continue
			dirarg=(); [ "$3" = . ] || dirarg=(--directory="$3")
			git -C "$d" am -q --keep-cr "${dirarg[@]}" "$P/$1"/*.patch
		done
		echo "$repo: plan9 $(git -C "$d" rev-parse --short plan9) on $t"
	done ;;
*)	die "usage: rust-patches.sh diff|apply" ;;
esac

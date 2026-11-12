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
# git am needs a committer; without a git identity (a new machine) a neutral one: the
# branches are local, each commit keeps its author from the patch
git config user.email >/dev/null 2>&1 || export GIT_COMMITTER_NAME=${GIT_COMMITTER_NAME:-nfront} GIT_COMMITTER_EMAIL=${GIT_COMMITTER_EMAIL:-nfront@localhost}
# git for a repository: rust's git data is rust.git beside rust/ (core.worktree=../rust):
# Rust's bootstrap takes rust/ for a source archive only without rust/.git, and only so
# finds its LLVM by the archive's git-commit-info
g(){ local r=$1; shift; if [ -d "$TOP/$r.git" ]; then git --git-dir="$TOP/$r.git" "$@"; else git -C "$TOP/$r" "$@"; fi; }
# the archive's tag of a repository (the one tag there is)
tag(){ g "$1" describe --tags --abbrev=0 archive; }
# the pathspec of a part
spec(){
	if [ "$2" != . ]; then echo "$2"
	elif [ "$1" = rust ]; then echo .; for d in "${SUBDIRS[@]}"; do echo ":(exclude)$d"; done
	else echo .; fi
}
ids(){ git patch-id --stable | awk '{print $1}' | sort; }

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
		g "$repo" format-patch -q --no-signature --zero-commit --full-index "${rel[@]}" -o "$P/$name" "$t..plan9" -- "${ps[@]}"
		echo "$name: $(ls "$P/$name" | grep -c '\.patch$') patches on $t"
	done ;;
apply)
	for repo in rust libc; do
		d=$TOP/$repo
		[ -d "$d/.git" ] || [ -d "$TOP/$repo.git" ] || die "$repo: no $repo/ (./fetch.sh)"
		t=$(tag "$repo")
		if g "$repo" rev-parse -q --verify plan9 >/dev/null; then
			if [ "$FORCE" != 1 ]; then
				want=$(for r in "${parts[@]}"; do set -- $r; [ "$2" = "$repo" ] && cat "$P/$1"/*.patch; done | ids "$d")
				have=$(g "$repo" log -p --reverse "$t..plan9" | ids "$d")
				[ "$have" = "$want" ] || [ -z "$have" ] ||
					die "$repo: plan9 differs from the patches (run diff first, or FORCE=1)"
			fi
			g "$repo" branch -f plan9-before-apply plan9
		fi
		g "$repo" checkout -q -B plan9 "$t"
		for r in "${parts[@]}"; do
			set -- $r
			[ "$2" = "$repo" ] || continue
			dirarg=(); [ "$3" = . ] || dirarg=(--directory="$3")
			g "$repo" am -q --keep-cr "${dirarg[@]}" "$P/$1"/*.patch
		done
		echo "$repo: plan9 $(g "$repo" rev-parse --short plan9) on $t"
	done ;;
*)	die "usage: rust-patches.sh diff|apply" ;;
esac

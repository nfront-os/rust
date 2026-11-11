#!/bin/bash
# The port's changes to Rust and its parts as patch series against upstream: for each repository
# (rust, its submodules cargo, backtrace and llvm-project, and libc) the commits of its branch
# plan9 on the upstream commit it is based on, as git format-patch writes them:
#   patches/<name>/BASE            "<upstream url> <base commit>"
#   patches/<name>/NNNN-*.patch    the commits, in order
#   scripts/rust-patches.sh diff   write them from the branches (after a rebase or a commit)
#   scripts/rust-patches.sh apply  make the branches plan9 from upstream and the patches
#                                  (a clone of each upstream repository must be there; it
#                                  fetches the base and applies with git am); it stops when
#                                  plan9 has commits the patches lack (run diff first; FORCE=1
#                                  overrides) and keeps the old plan9 as plan9-before-apply
# rust's bases of its submodules are the commits upstream's rust pins; libc's is the upstream
# branch libc-0.2's commit the branch is on.
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
P=$TOP/patches
# name, directory, upstream url, base (empty: from rust's upstream pin of the submodule)
repos=(
	"rust $TOP/rust https://github.com/rust-lang/rust.git origin/main"
	"cargo $TOP/rust/src/tools/cargo https://github.com/rust-lang/cargo.git"
	"backtrace $TOP/rust/library/backtrace https://github.com/rust-lang/backtrace-rs.git"
	"llvm-project $TOP/rust/src/llvm-project https://github.com/rust-lang/llvm-project.git"
	"libc $TOP/libc https://github.com/rust-lang/libc.git origin/libc-0.2"
)
die(){ echo "rust-patches: $*" >&2; exit 1; }
# the patch ids of a series, sorted (equal for the same changes, whatever the commit ids)
ids(){ git -C "$1" patch-id --stable | awk '{print $1}' | sort; }

# the submodule's commit in rust's upstream base
pin(){
	local rb
	rb=$(git -C "$TOP/rust" merge-base plan9 origin/main)
	git -C "$TOP/rust" ls-tree "$rb" "$1" | awk '{print $3}'
}

case $1 in
diff)
	for r in "${repos[@]}"; do
		set -- $r
		name=$1 dir=$2 url=$3 base=$4
		case $name in
		cargo) base=$(pin src/tools/cargo) ;;
		backtrace) base=$(pin library/backtrace) ;;
		llvm-project) base=$(pin src/llvm-project) ;;
		*) base=$(git -C "$dir" merge-base plan9 "$base") ;;
		esac
		git -C "$dir" merge-base --is-ancestor "$base" plan9 || die "$name: plan9 is not on $base"
		rm -rf "$P/$name"
		mkdir -p "$P/$name"
		echo "$url $base" > "$P/$name/BASE"
		git -C "$dir" format-patch -q --no-signature --zero-commit -o "$P/$name" "$base..plan9"
		echo "$name: $(ls "$P/$name" | grep -c '\.patch$') patches on ${base:0:12}"
	done ;;
apply)
	for r in "${repos[@]}"; do
		set -- $r
		name=$1 dir=$2
		read url base < "$P/$name/BASE"
		[ -d "$dir/.git" ] || [ -f "$dir/.git" ] || die "$name: no clone in $dir (git clone $url)"
		git -C "$dir" cat-file -e "$base^{commit}" 2>/dev/null || git -C "$dir" fetch "$url" "$base"
		if git -C "$dir" rev-parse -q --verify plan9 >/dev/null; then
			if [ "$FORCE" != 1 ] && [ "$(git -C "$dir" log -p --reverse "$base..plan9" | ids "$dir")" != \
				"$(cat "$P/$name"/*.patch | ids "$dir")" ]; then
				die "$name: plan9 differs from patches/$name (run diff first, or FORCE=1)"
			fi
			git -C "$dir" branch -f plan9-before-apply plan9
		fi
		git -C "$dir" checkout -q -B plan9 "$base"
		git -C "$dir" am -q --keep-cr "$P/$name"/*.patch
		echo "$name: plan9 $(git -C "$dir" rev-parse --short plan9)"
	done ;;
*)	die "usage: rust-patches.sh diff|apply" ;;
esac

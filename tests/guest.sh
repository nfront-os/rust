#!/bin/bash
# Run programs in the 9front guest (../9front's scripts/guest/boot-cli.sh): the files
# given are copied to /tests; each further argument is one rc command line.
# -x and -d add a file or a directory to the root (ufs, the host's over 9P: slow for big
# trees); -D puts a directory on an ext4 disk instead (mkfs.ext4 -d, as the tree's
# build-self.sh; 9front's ext4srv), bound at /guest/dir before the commands, with room
# for builds in /tmp (/n/disk/work; DISKMB more megabytes, 4096). SMP: processors (4).
# KEEPDISK=file: the disk image is kept there (debugfs -R 'dump /work/x x' file).
# usage: tests/guest.sh [-x /guest/path=hostfile]... [-d /guest/dir=hostdir]...
#        [-D /guest/dir=hostdir]... -f file... -- cmd...
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
PLAN9=$(cd "${PLAN9:-$TOP/../9front}" && pwd)
mkdir -p "$TOP/build"
T=$(mktemp -d -p "$TOP/build")	# beside the trees -D links from
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/extra/tests" "$T/disk/work"
binds=() nd=0
while [ $# -gt 0 ] && [ "$1" != -- ]; do
	case $1 in
	-f) ;;
	-x) shift; dst=${1%%=*}; mkdir -p "$T/extra$(dirname "$dst")"; cp "${1#*=}" "$T/extra$dst" ;;
	-d) shift; dst=${1%%=*}; mkdir -p "$T/extra$dst"; cp -r "${1#*=}"/. "$T/extra$dst/" ;;
	-D) shift; dst=${1%%=*}; nd=$((nd+1)); mkdir -p "$T/extra$dst"
		cp -rL --link "${1#*=}" "$T/disk/d$nd" 2>/dev/null || cp -rL "${1#*=}" "$T/disk/d$nd"
		binds+=("bind /n/disk/d$nd $dst") ;;
	*) cp "$1" "$T/extra/tests/" ;;
	esac
	shift
done
[ "$1" = -- ] && shift
disk=()
if [ $nd -gt 0 ]; then
	IMG=$T/disk.img
	mb=$(( $(du -smL "$T/disk" | cut -f1) + ${DISKMB:-4096} ))
	mkfs.ext4 -q -F -L disk -O ^has_journal,^orphan_file,^metadata_csum,^metadata_csum_seed \
		-E root_owner=$(id -u):$(id -g) -d "$T/disk" "$IMG" ${mb}M
	rm -rf "$T/disk"
	disk=(-drive "file=$IMG,format=raw,if=virtio,cache=unsafe")
	# ext4srv keeps writes in its cache: sync at the end (KEEPDISK reads the image)
	set -- "ext4srv -S -n disk; mount -c /srv/disk /n/disk /dev/sdF0/data; bind -c /n/disk/work /tmp" \
		"${binds[@]}" "$@" "echo sync >> /srv/disk.cmd; sleep 2"
fi
[ -z "$KEEPDISK" ] || [ $nd -eq 0 ] || trap 'mv "$IMG" "$KEEPDISK"; rm -rf "$T"' EXIT
cd "$PLAN9"
# -smp through QEMU_EXTRA: any tree's boot-cli.sh takes that (SMP= keeps one that knows
# SMP from adding its own)
smp=${SMP:-4}
# its own process group: if this script ends early (an outer timeout's TERM), the guest
# (QEMU, ufs) goes with it, or it keeps the ports and the next guest fails
SMP= UFS_EXTRA=$T/extra QEMU_EXTRA="-smp $smp ${disk[*]} $QEMU_EXTRA" setsid timeout ${GUEST_TIMEOUT:-600} \
	bash scripts/guest/boot-cli.sh "$@" > "$T/log" 2>&1 &
guest=$!
trap 'kill -TERM -- -$guest 2>/dev/null; exit 1' TERM INT
wait $guest || true
tr -d '\r' < "$T/log" | sed -n '/^init: starting/,$p'

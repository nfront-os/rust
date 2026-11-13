#!/bin/bash
# LLVM (rust/src/llvm-project = llvm-project's branch plan9, Rust's fork) for 9front amd64:
# libraries only, X86, static, C++ against APE (cmake/plan9.cmake), the
# tablegens for Linux from the same sources (build/llvm-native-tools, made first). Out: build/llvm-plan9 (bin/llvm-config
# is a 9front program: scripts/llvm-config-plan9.sh makes one for Linux, at the end).
# The threads are APE's, in libap: CMake's FindThreads is told so (its probes cannot link).
# objtype=386: for 9front 386 (i686-unknown-plan9), out build/llvm-plan9.386; objtype=arm64:
# arm64 (aarch64-unknown-plan9, the AArch64 backend), out build/llvm-plan9.arm64.
# usage: [objtype=386|arm64] scripts/build-llvm.sh [ninja targets]
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
. "$TOP/scripts/nfront-paths.sh"	# GCC_DIR (the distribution's paths.env)
export GCC9BUILD=${GCC9BUILD:-$GCC_DIR/build}	# cmake/plan9.cmake: the cross compilers
case ${objtype:=amd64} in
amd64)	B=$TOP/build/llvm-plan9 T=x86_64-unknown-plan9 L= BACKEND=X86 ;;
386)	B=$TOP/build/llvm-plan9.386 T=i686-unknown-plan9 L=.386 BACKEND=X86 ;;
arm64)	B=$TOP/build/llvm-plan9.arm64 T=aarch64-unknown-plan9 L=.arm64 BACKEND=AArch64 ;;
*)	echo "build-llvm.sh: objtype=$objtype: no target" >&2; exit 1 ;;
esac
export objtype
N=$TOP/build/llvm-native-tools
if [ ! -x "$N/bin/llvm-tblgen" ] || [ ! -f "$N/lib/libLLVMTargetParser.a" ]; then
	mkdir -p "$N"
	(cd "$N" && cmake -G Ninja "$TOP/rust/src/llvm-project/llvm" -DCMAKE_BUILD_TYPE=Release \
		-DLLVM_TARGETS_TO_BUILD=X86 -DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
		-DLLVM_INCLUDE_EXAMPLES=OFF > "$TOP/logs/llvm-native-cmake.log" 2>&1 &&
		ninja llvm-tblgen llvm-min-tblgen LLVMTargetParser LLVMSupport LLVMDemangle \
			> "$TOP/logs/llvm-native-build.log" 2>&1)	# the libraries: llvm-config-plan9.sh's
fi
mkdir -p "$B"
cd "$B"
cmake -G Ninja "$TOP/rust/src/llvm-project/llvm" \
	-DCMAKE_TOOLCHAIN_FILE="$TOP/cmake/plan9.cmake" \
	-DCMAKE_BUILD_TYPE=Release \
	-DLLVM_TARGETS_TO_BUILD=$BACKEND \
	-DLLVM_HOST_TRIPLE=$T \
	-DLLVM_DEFAULT_TARGET_TRIPLE=$T \
	-DLLVM_ENABLE_THREADS=ON \
	-DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF \
	-DLLVM_ENABLE_LIBEDIT=OFF -DLLVM_ENABLE_LIBPFM=OFF -DLLVM_ENABLE_BINDINGS=OFF \
	-DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
	-DLLVM_INCLUDE_DOCS=OFF -DLLVM_BUILD_TOOLS=OFF -DLLVM_BUILD_UTILS=OFF \
	-DLLVM_ENABLE_PIC=OFF -DLLVM_ENABLE_ASSERTIONS=OFF -DLLVM_ENABLE_PLUGINS=OFF \
	-DLLVM_ENABLE_BACKTRACES=OFF -DLLVM_ENABLE_CRASH_OVERRIDES=OFF \
	-DLLVM_NATIVE_TOOL_DIR="$N/bin" \
	-DLLVM_ENABLE_ONDISK_CAS=OFF -DCMAKE_DISABLE_FIND_PACKAGE_Backtrace=ON \
	-DCMAKE_THREAD_LIBS_INIT= -DCMAKE_HAVE_THREADS_LIBRARY=1 -DCMAKE_USE_PTHREADS_INIT=1 \
	-DThreads_FOUND=TRUE -DCMAKE_HAVE_LIBC_PTHREAD=1 \
	> "$TOP/logs/llvm-cmake$L.log" 2>&1
ninja -k 0 "$@" > "$TOP/logs/llvm-build$L.log" 2>&1
# bootstrap's llvm-config ([target.T] llvm-config), a Linux program; not for some targets only
[ $# -gt 0 ] || "$TOP/scripts/llvm-config-plan9.sh"

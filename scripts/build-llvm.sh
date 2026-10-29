#!/bin/bash
# LLVM (rust/src/llvm-project = llvm-project's branch plan9, Rust's fork) for 9front amd64:
# libraries only, X86, static, C++ against APE (cmake/plan9.cmake), the
# tablegens for Linux from the same sources (build/llvm-native-tools, made first). Out: build/llvm-plan9 (bin/llvm-config
# is a 9front program: scripts/llvm-config-plan9 answers for it on Linux).
# usage: scripts/build-llvm.sh [ninja targets]
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
B=$TOP/build/llvm-plan9
N=$TOP/build/llvm-native-tools
if [ ! -x "$N/bin/llvm-tblgen" ]; then
	mkdir -p "$N"
	(cd "$N" && cmake -G Ninja "$TOP/rust/src/llvm-project/llvm" -DCMAKE_BUILD_TYPE=Release \
		-DLLVM_TARGETS_TO_BUILD=X86 -DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
		-DLLVM_INCLUDE_EXAMPLES=OFF > "$TOP/logs/llvm-native-cmake.log" 2>&1 &&
		ninja llvm-tblgen llvm-min-tblgen > "$TOP/logs/llvm-native-build.log" 2>&1)
fi
mkdir -p "$B"
cd "$B"
[ -f build.ninja ] || cmake -G Ninja "$TOP/rust/src/llvm-project/llvm" \
	-DCMAKE_TOOLCHAIN_FILE="$TOP/cmake/plan9.cmake" \
	-DCMAKE_BUILD_TYPE=Release \
	-DLLVM_TARGETS_TO_BUILD=X86 \
	-DLLVM_HOST_TRIPLE=x86_64-unknown-plan9 \
	-DLLVM_DEFAULT_TARGET_TRIPLE=x86_64-unknown-plan9 \
	-DLLVM_ENABLE_THREADS=ON \
	-DLLVM_ENABLE_ZLIB=OFF -DLLVM_ENABLE_ZSTD=OFF -DLLVM_ENABLE_LIBXML2=OFF \
	-DLLVM_ENABLE_LIBEDIT=OFF -DLLVM_ENABLE_LIBPFM=OFF -DLLVM_ENABLE_BINDINGS=OFF \
	-DLLVM_INCLUDE_TESTS=OFF -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF \
	-DLLVM_INCLUDE_DOCS=OFF -DLLVM_BUILD_TOOLS=OFF -DLLVM_BUILD_UTILS=OFF \
	-DLLVM_ENABLE_PIC=OFF -DLLVM_ENABLE_ASSERTIONS=OFF -DLLVM_ENABLE_PLUGINS=OFF \
	-DLLVM_ENABLE_BACKTRACES=OFF -DLLVM_ENABLE_CRASH_OVERRIDES=OFF \
	-DLLVM_NATIVE_TOOL_DIR="$N/bin" \
	-DLLVM_ENABLE_ONDISK_CAS=OFF -DCMAKE_DISABLE_FIND_PACKAGE_Backtrace=ON \
	> "$TOP/logs/llvm-cmake.log" 2>&1
ninja -k 0 "$@" > "$TOP/logs/llvm-build.log" 2>&1

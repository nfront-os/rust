#!/bin/bash
# llvm-config of build/llvm-plan9 for Linux: llvm-config.cpp with the plan9 build's tables
# (BuildVariables.inc, LibraryDependencies.inc), linked with build/llvm-native-tools' Support.
# It finds its libraries relative to itself: build/llvm-plan9/bin/llvm-config, for bootstrap's
# [target.x86_64-unknown-plan9] llvm-config. objtype=386: build/llvm-plan9.386's
# ([target.i686-unknown-plan9]).
set -e
TOP=$(cd "$(dirname "$0")/.." && pwd)
case ${objtype:-amd64} in
386)	B=$TOP/build/llvm-plan9.386 ;;
arm64)	B=$TOP/build/llvm-plan9.arm64 ;;
*)	B=$TOP/build/llvm-plan9 ;;
esac
N=$TOP/build/llvm-native-tools
S=$TOP/rust/src/llvm-project/llvm
mkdir -p "$B/bin"
g++ -O1 -std=c++17 -o "$B/bin/llvm-config" "$S/tools/llvm-config/llvm-config.cpp" \
	-I"$B/tools/llvm-config" -I"$N/include" -I"$S/include" \
	-L"$N/lib" -lLLVMTargetParser -lLLVMSupport -lLLVMDemangle -lpthread
"$B/bin/llvm-config" --version

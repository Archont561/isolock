#!/bin/sh
# Run inside the restored sandbox; locate LLVM tools beside this exact Rust compiler.
set -eu
sysroot=$(rustc --print sysroot)
host=$(rustc -vV | sed -n 's/^host: //p')
[ -n "$host" ] || { echo 'cannot determine Rust host for coverage' >&2; exit 1; }
LLVM_COV=$sysroot/lib/rustlib/$host/bin/llvm-cov
LLVM_PROFDATA=$sysroot/lib/rustlib/$host/bin/llvm-profdata
export LLVM_COV LLVM_PROFDATA
[ -x "$LLVM_COV" ] && [ -x "$LLVM_PROFDATA" ] || { echo 'restored Rust compiler has no matching LLVM coverage tools' >&2; exit 1; }
mkdir -p coverage
exec cargo llvm-cov --workspace --all-features --offline --lcov --output-path coverage/lcov.info --fail-under-lines 90 "$@"

#!/bin/bash
# Builds a crust app for a web page: WASI, then asyncify, so a wait for a
# key pauses the app and hands the page its thread back (see term.js).
#
#   web/build.sh ../rpnx rpnx.wasm
#
# Needs the wasm32-wasip1 target (rustup target add wasm32-wasip1) and
# binaryen's wasm-opt on PATH or in WASM_OPT.
set -e
app=$(cd "$1" && pwd); out=$(realpath -m "$2")
bin=$(basename "$app")
opt=${WASM_OPT:-wasm-opt}
(cd "$app" && PATH="/usr/bin:$PATH" cargo build -q --release --target wasm32-wasip1)
# The features Rust's wasm32 targets use by default; every current browser has them.
"$opt" "$app/target/wasm32-wasip1/release/$bin.wasm" -O2 --asyncify \
  --enable-bulk-memory --enable-bulk-memory-opt --enable-nontrapping-float-to-int --enable-sign-ext \
  --enable-mutable-globals --enable-multivalue --enable-reference-types \
  --pass-arg=asyncify-imports@crust.key,wasi_snapshot_preview1.poll_oneoff \
  --strip-debug -o "$out"
echo "$out $(stat -c %s "$out")"

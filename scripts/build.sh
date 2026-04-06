#!/usr/bin/env bash

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
export CARBONYL_ROOT="$(cd -- "$script_dir/.." && pwd -P)"
export INSTALL_DEPOT_TOOLS="true"

cd "$CARBONYL_ROOT"
source scripts/env.sh

target="$1"
if [ ! -z "$target" ]; then
    shift
fi

cpu=""
if [ $# -gt 0 ] && [[ "$1" != -* ]]; then
    cpu="$1"
    shift
fi

triple=$(scripts/platform-triple.sh "$cpu")

if [ -z "$CARBONYL_SKIP_CARGO_BUILD" ]; then
    if [ -z "$MACOSX_DEPLOYMENT_TARGET" ]; then
        export MACOSX_DEPLOYMENT_TARGET=10.13
    fi

    cargo build --target "$triple" --release
fi

if [ -f "build/$triple/release/libcarbonyl.dylib" ]; then
    cp "build/$triple/release/libcarbonyl.dylib" "$CHROMIUM_SRC/out/$target"
    install_name_tool \
        -id @executable_path/libcarbonyl.dylib \
        "build/$triple/release/libcarbonyl.dylib"
else
    cp "build/$triple/release/libcarbonyl.so" "$CHROMIUM_SRC/out/$target"
fi

cd "$CHROMIUM_SRC"

if [ -x "$CHROMIUM_SRC/third_party/ninja/ninja" ]; then
    ninja_bin="$CHROMIUM_SRC/third_party/ninja/ninja"
else
    ninja_bin="ninja"
fi

"$ninja_bin" -C "out/$target" headless:headless_shell "$@"

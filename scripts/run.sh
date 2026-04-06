#!/usr/bin/env bash

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
export CARBONYL_ROOT="$(cd -- "$script_dir/.." && pwd -P)"

source "$CARBONYL_ROOT/scripts/env.sh"

target="$1"
if [ -z "$target" ]; then
    echo "Usage: ./scripts/run.sh <target> [headless_shell args...]"
    exit 1
fi
shift

"$CHROMIUM_SRC/out/$target/headless_shell" "$@"

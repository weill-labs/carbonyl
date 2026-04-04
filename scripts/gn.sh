#!/usr/bin/env bash

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
export CARBONYL_ROOT="$(cd -- "$script_dir/.." && pwd -P)"
export INSTALL_DEPOT_TOOLS="true"

source "$CARBONYL_ROOT/scripts/env.sh"

(
    cd "$CHROMIUM_SRC" &&
    gn "$@"
)

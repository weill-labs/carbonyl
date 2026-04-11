#!/usr/bin/env bash

set -eo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
export CARBONYL_ROOT="$(cd -- "$script_dir/.." && pwd -P)"
export INSTALL_DEPOT_TOOLS="false"

cd "$CARBONYL_ROOT"
source "$CARBONYL_ROOT/scripts/env.sh"

usage() {
    cat <<EOF
Usage: ./scripts/package.sh <build-dir> [--cpu <cpu>] [--version <version>] [--output-dir <dir>] [--dry-run]

Bundle a built Carbonyl runtime into a GitHub Releases tarball.

Arguments:
  <build-dir>            Completed Chromium output directory containing headless_shell

Options:
  --cpu <cpu>            Target CPU for scripts/platform-triple.sh (default: host CPU)
  --version <version>    Version used in the archive name (default: package.json version)
  --output-dir <dir>     Destination directory for the tarball (default: build/packages)
  --dry-run              Validate inputs and print planned actions without writing files
  -h, --help             Show this message
EOF
}

die() {
    echo "$*" >&2
    exit 1
}

require_value() {
    local flag="$1"
    local value="${2:-}"

    if [ -z "$value" ]; then
        die "Missing value for $flag"
    fi
}

package_version() {
    sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$CARBONYL_ROOT/package.json" | head -n 1
}

platform_from_triple() {
    local triple="$1"

    case "$triple" in
        x86_64-unknown-linux-gnu)
            echo -n "linux-amd64"
            ;;
        aarch64-unknown-linux-gnu)
            echo -n "linux-arm64"
            ;;
        x86_64-apple-darwin)
            echo -n "macos-amd64"
            ;;
        aarch64-apple-darwin)
            echo -n "macos-arm64"
            ;;
        *)
            die "Unsupported target triple: $triple"
            ;;
    esac
}

lib_extension_from_triple() {
    local triple="$1"

    case "$triple" in
        *-apple-darwin)
            echo -n "dylib"
            ;;
        *)
            echo -n "so"
            ;;
    esac
}

print_copy_plan() {
    local archive_path="$1"
    shift
    local entries=("$@")

    echo "Package build dir: $build_dir"
    echo "Archive path: $archive_path"
    echo "Bundled files:"

    for entry in "${entries[@]}"; do
        echo "  - ${entry#$build_dir/}"
    done
}

cpu=""
version=""
output_dir="$CARBONYL_ROOT/build/packages"
build_dir=""
dry_run="false"

while [ $# -gt 0 ]; do
    case "$1" in
        --cpu)
            require_value "$1" "${2:-}"
            cpu="$2"
            shift 2
            ;;
        --version)
            require_value "$1" "${2:-}"
            version="$2"
            shift 2
            ;;
        --output-dir)
            require_value "$1" "${2:-}"
            output_dir="$2"
            shift 2
            ;;
        --dry-run)
            dry_run="true"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            die "Unknown option: $1"
            ;;
        *)
            if [ -n "$build_dir" ]; then
                die "Build directory already set to $build_dir"
            fi

            build_dir="$1"
            shift
            ;;
    esac
done

if [ -z "$build_dir" ]; then
    usage >&2
    exit 1
fi

if [ ! -d "$build_dir" ]; then
    die "Build directory not found: $build_dir"
fi

build_dir="$(cd -- "$build_dir" && pwd -P)"
output_dir="$(mkdir -p "$output_dir" && cd -- "$output_dir" && pwd -P)"

if [ -z "$version" ]; then
    version="$(package_version)"
fi

if [ -z "$version" ]; then
    die "Could not determine package version"
fi

triple="$(scripts/platform-triple.sh "$cpu")"
platform="$(platform_from_triple "$triple")"
lib_ext="$(lib_extension_from_triple "$triple")"
archive_name="carbonyl-$version-$platform.tar.gz"
archive_path="$output_dir/$archive_name"
package_root="${archive_name%.tar.gz}"
package_out_dir="$package_root/chromium/src/out/carbonyl"

required_files=(
    "$build_dir/headless_shell"
    "$build_dir/icudtl.dat"
    "$build_dir/libcarbonyl.$lib_ext"
    "$build_dir/libEGL.$lib_ext"
    "$build_dir/libGLESv2.$lib_ext"
)

for file in "${required_files[@]}"; do
    if [ ! -f "$file" ]; then
        die "Required file not found: $file"
    fi
done

shopt -s nullglob
required_v8_snapshots=("$build_dir"/v8_context_snapshot*.bin)

if [ ${#required_v8_snapshots[@]} -eq 0 ]; then
    shopt -u nullglob
    die "Required file not found: $build_dir/v8_context_snapshot*.bin"
fi

copy_entries=(
    "${required_files[@]}"
    "${required_v8_snapshots[@]}"
)

optional_files=(
    "$build_dir/snapshot_blob.bin"
    "$build_dir/libvk_swiftshader.$lib_ext"
    "$build_dir/libvulkan.$lib_ext"
    "$build_dir/libvulkan.so.1"
    "$build_dir/vk_swiftshader_icd.json"
)

for file in "${optional_files[@]}"; do
    if [ -f "$file" ]; then
        copy_entries+=("$file")
    fi
done

optional_paks=("$build_dir"/*.pak)
for file in "${optional_paks[@]}"; do
    if [ -f "$file" ]; then
        copy_entries+=("$file")
    fi
done
shopt -u nullglob

if [ -d "$build_dir/locales" ]; then
    copy_entries+=("$build_dir/locales")
fi

if [ "$dry_run" == "true" ]; then
    print_copy_plan "$archive_path" "${copy_entries[@]}"
    exit 0
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/$package_out_dir"
cp "$CARBONYL_ROOT/scripts/carbonyl" "$tmp_dir/$package_root/carbonyl"
chmod 755 "$tmp_dir/$package_root/carbonyl"
cp "$CARBONYL_ROOT/readme.md" "$tmp_dir/$package_root/readme.md"
cp "$CARBONYL_ROOT/license.md" "$tmp_dir/$package_root/license.md"

for entry in "${copy_entries[@]}"; do
    cp -R "$entry" "$tmp_dir/$package_out_dir"
done

tar -C "$tmp_dir" -czf "$archive_path" "$package_root"

echo "$archive_path"

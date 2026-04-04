#!/usr/bin/env bash

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
export CARBONYL_ROOT="$(cd -- "$script_dir/.." && pwd -P)"

source "$CARBONYL_ROOT/scripts/env.sh"

cd "$CHROMIUM_SRC"

chromium_upstream="5a40bc538bcbf1b9f4010f026d38a8afa2821905"
skia_upstream="abbe599fb3c0ef2fa82bfadbb0ddcd321f22faf0"
webrtc_upstream="9179833d210d105aede5d4ec516734a6bd1ef2e8"

apply_patch_series() {
    local repo_root="$1"
    local upstream="$2"
    local patch_dir="$3"
    local label="$4"
    local -a patches=()

    if [[ -d "$patch_dir" ]]; then
        shopt -s nullglob
        patches=("$patch_dir"/*.patch)
        shopt -u nullglob
    fi

    if [[ "${#patches[@]}" -eq 0 ]]; then
        echo "No $label patches to apply"
        return
    fi

    git -C "$repo_root" checkout "$upstream"
    git -C "$repo_root" am -3 --committer-date-is-author-date "${patches[@]}"
    "$CARBONYL_ROOT/scripts/restore-mtime.sh" "$upstream"
}

save_patch_series() {
    local repo_root="$1"
    local upstream="$2"
    local patch_dir="$3"
    local label="$4"

    rm -rf "$patch_dir"

    if [[ -z "$(git -C "$repo_root" rev-list --max-count=1 "$upstream..HEAD")" ]]; then
        echo "No $label patches to save"
        return
    fi

    git -C "$repo_root" format-patch --no-signature --output-directory "$patch_dir" "$upstream"
}

if [[ "$1" == "apply" ]]; then
    echo "Stashing Chromium changes.."
    git add -A .
    git stash

    echo "Applying Chromium patches.."
    apply_patch_series "$CHROMIUM_SRC" "$chromium_upstream" "$CARBONYL_ROOT/chromium/patches/chromium" "Chromium"

    echo "Stashing Skia changes.."
    cd "$CHROMIUM_SRC/third_party/skia"
    git add -A .
    git stash

    echo "Applying Skia patches.."
    apply_patch_series "$CHROMIUM_SRC/third_party/skia" "$skia_upstream" "$CARBONYL_ROOT/chromium/patches/skia" "Skia"

    echo "Stashing WebRTC changes.."
    cd "$CHROMIUM_SRC/third_party/webrtc"
    git add -A .
    git stash

    echo "Applying WebRTC patches.."
    apply_patch_series "$CHROMIUM_SRC/third_party/webrtc" "$webrtc_upstream" "$CARBONYL_ROOT/chromium/patches/webrtc" "WebRTC"

    echo "Patches successfully applied"
elif [[ "$1" == "save" ]]; then
    if [[ -d carbonyl ]]; then
        git add -A carbonyl
    fi

    echo "Updating Chromium patches.."
    save_patch_series "$CHROMIUM_SRC" "$chromium_upstream" "$CARBONYL_ROOT/chromium/patches/chromium" "Chromium"

    echo "Updating Skia patches.."
    save_patch_series "$CHROMIUM_SRC/third_party/skia" "$skia_upstream" "$CARBONYL_ROOT/chromium/patches/skia" "Skia"

    echo "Updating WebRTC patches.."
    save_patch_series "$CHROMIUM_SRC/third_party/webrtc" "$webrtc_upstream" "$CARBONYL_ROOT/chromium/patches/webrtc" "WebRTC"

    echo "Patches successfully updated"
else
    echo "Unknown argument: $1"

    exit 2
fi

#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -f engine/Cargo.toml ]]; then git submodule update --init --recursive; fi
if ! git -C engine apply --reverse --check ../patches/ios.patch 2>/dev/null; then
    git -C engine apply --check ../patches/ios.patch
    git -C engine apply ../patches/ios.patch
fi
python3 scripts/prepare-vm.py

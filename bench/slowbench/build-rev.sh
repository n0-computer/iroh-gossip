#!/bin/bash
# Builds the harness against a revision of iroh-gossip.
#
# Usage: build-rev.sh <label> <git-rev> [workdir]
# Leaves the binary at <workdir>/bins/<label>. Each revision gets its own copy
# of the harness and its own target dir: a shared build dir (as with kache)
# otherwise hands back the same binary for every revision.
set -euo pipefail
label=$1; rev=$2; work=${3:-/tmp/slowbench}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
mkdir -p "$work/revs" "$work/bins"
[ -d "$work/revs/$label" ] || git -C "$repo" worktree add -q --detach "$work/revs/$label" "$rev"
rm -rf "$work/h-$label"; mkdir -p "$work/h-$label"
cp -r "$here/src" "$here/Cargo.lock" "$work/h-$label/"
sed "s|^iroh-gossip = { path = \"../..\" }|iroh-gossip = { path = \"$work/revs/$label\" }|" \
  "$here/Cargo.toml" > "$work/h-$label/Cargo.toml"
(cd "$work/h-$label" && CARGO_TARGET_DIR="$work/t-$label" cargo build -q --release)
cp "$work/t-$label/release/slowbench" "$work/bins/$label"
echo "$work/bins/$label"

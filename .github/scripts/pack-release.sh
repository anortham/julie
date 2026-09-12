#!/usr/bin/env bash
# Usage:
#   pack-release.sh <target> <version> <release-dir> <out-dir>
#     Writes <out-dir>/julie-v<version>-<target>.tar.gz (.zip on Windows)
#     with julie-server and the pinned julie-semantic-sidecar files at the
#     archive root. The last stdout line is the archive path.
#   pack-release.sh --dry-run <target>
#     Prints the pinned sidecar asset name and sha256. No network.
# Exit codes: 0 ok, 1 download or checksum failure, 2 bad arguments.
set -euo pipefail

SIDECAR_VERSION=0.1.0
SIDECAR_REPO=anortham/julie-semantic-sidecar

resolve_sidecar() {
  case "$1" in
    aarch64-apple-darwin)
      ASSET="julie-semantic-sidecar-${SIDECAR_VERSION}-aarch64-apple-darwin-metal-portable.tar.gz"
      SHA256=bd84211306145690c1033338c775c5f5af7bcf0752d8e6b25c579dbd082e0ab1 ;;
    x86_64-apple-darwin)
      ASSET="julie-semantic-sidecar-${SIDECAR_VERSION}-x86_64-apple-darwin-metal-portable.tar.gz"
      SHA256=c4e996abdd711efde1075af0cd222643a298354ec10f03f1f7c3bee9f70a8bdf ;;
    x86_64-unknown-linux-gnu)
      ASSET="julie-semantic-sidecar-${SIDECAR_VERSION}-x86_64-unknown-linux-gnu-vulkan-portable.tar.gz"
      SHA256=14b369076776fc7e7ed0ee2a8d8261311e09b634bdc7d57eed928c4fcfa61212 ;;
    x86_64-pc-windows-msvc)
      ASSET="julie-semantic-sidecar-${SIDECAR_VERSION}-x86_64-pc-windows-msvc-vulkan-portable.zip"
      SHA256=659c952b405a087b98775e9fc68223b067955b61f525d7647e50087fe052972d ;;
    *)
      echo "pack-release.sh: unknown target '$1'" >&2
      exit 2 ;;
  esac
}

if [ "${1:-}" = "--dry-run" ]; then
  [ $# -eq 2 ] || { echo "usage: pack-release.sh --dry-run <target>" >&2; exit 2; }
  resolve_sidecar "$2"
  echo "${SHA256}  ${ASSET}"
  exit 0
fi

[ $# -eq 4 ] || { echo "usage: pack-release.sh <target> <version> <release-dir> <out-dir>" >&2; exit 2; }
TARGET=$1
VERSION=$2
RELEASE_DIR=$3
OUT_DIR=$4
resolve_sidecar "$TARGET"

command -v gh >/dev/null || { echo "pack-release.sh: gh is not installed" >&2; exit 1; }

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
STAGE="$WORK/stage"
mkdir -p "$STAGE" "$OUT_DIR"
OUT_DIR=$(cd "$OUT_DIR" && pwd)

gh release download "v${SIDECAR_VERSION}" --repo "$SIDECAR_REPO" --pattern "$ASSET" --dir "$WORK"
if command -v sha256sum >/dev/null; then
  ACTUAL=$(sha256sum "$WORK/$ASSET" | cut -d' ' -f1)
else
  ACTUAL=$(shasum -a 256 "$WORK/$ASSET" | cut -d' ' -f1)
fi
if [ "$ACTUAL" != "$SHA256" ]; then
  echo "pack-release.sh: sha256 mismatch for $ASSET" >&2
  echo "  expected: $SHA256" >&2
  echo "  actual:   $ACTUAL" >&2
  exit 1
fi

case "$ASSET" in
  *.zip) unzip -q "$WORK/$ASSET" -d "$STAGE" ;;
  *) tar xzf "$WORK/$ASSET" -C "$STAGE" ;;
esac
mv "$STAGE/package-manifest.json" "$STAGE/sidecar-package-manifest.json"
if [ "$TARGET" = "x86_64-pc-windows-msvc" ]; then
  cp "$RELEASE_DIR/julie-server.exe" "$STAGE/"
else
  cp "$RELEASE_DIR/julie-server" "$STAGE/"
fi
find -P "$STAGE" -type l -print -quit | grep -q . && {
  echo "pack-release.sh: sidecar package contains a symlink" >&2
  exit 1
}
python3 - "$STAGE" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

stage = Path(sys.argv[1])
manifest_path = stage / "sidecar-package-manifest.json"
source_manifest = json.loads(manifest_path.read_text())
files = []
for path in sorted(stage.iterdir()):
    if not path.is_file():
        raise SystemExit(f"sidecar package is not flat: {path.name}")
    if path.name == manifest_path.name:
        continue
    files.append({"path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
manifest_path.write_text(json.dumps({"source": source_manifest, "files": files}, indent=2) + "\n")
PY
if [ "$TARGET" = "x86_64-pc-windows-msvc" ]; then
  ARCHIVE="$OUT_DIR/julie-v${VERSION}-${TARGET}.zip"
  rm -f "$ARCHIVE"
  (cd "$STAGE" && 7z a "$ARCHIVE" ./* >/dev/null)
else
  ARCHIVE="$OUT_DIR/julie-v${VERSION}-${TARGET}.tar.gz"
  (cd "$STAGE" && tar -czf "$ARCHIVE" -- *)
fi
echo "$ARCHIVE"

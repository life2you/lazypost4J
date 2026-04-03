#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/update_homebrew_formula.sh <version>

Examples:
  scripts/update_homebrew_formula.sh v0.1.1
  scripts/update_homebrew_formula.sh 0.1.1

Environment variables:
  TAP_REPO_PATH   Path to the homebrew-lazypost4j repository.
                  Default: ../homebrew-lazypost4j (sibling of this repo)
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 1
fi

VERSION_INPUT="$1"
if [[ "$VERSION_INPUT" == v* ]]; then
  TAG="$VERSION_INPUT"
else
  TAG="v${VERSION_INPUT}"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
DEFAULT_TAP_REPO="$(cd "${ROOT_DIR}/.." && pwd)/homebrew-lazypost4j"
TAP_REPO_PATH="${TAP_REPO_PATH:-$DEFAULT_TAP_REPO}"
FORMULA_PATH="${TAP_REPO_PATH}/Formula/lazypost.rb"

if [[ ! -f "$FORMULA_PATH" ]]; then
  echo "Formula not found: $FORMULA_PATH" >&2
  exit 1
fi

ARCHIVE_URL="https://github.com/life2you/lazypost4J/archive/refs/tags/${TAG}.tar.gz"
TMP_ARCHIVE="/tmp/lazypost-${TAG}.tar.gz"

echo "Downloading ${ARCHIVE_URL}"
curl -fL "$ARCHIVE_URL" -o "$TMP_ARCHIVE"

SHA256="$(shasum -a 256 "$TMP_ARCHIVE" | awk '{print $1}')"
echo "sha256=${SHA256}"

FORMULA_PATH="$FORMULA_PATH" TAG="$TAG" SHA256_VALUE="$SHA256" python3 - <<'PY'
import os
import pathlib
import re
import sys

formula_path = pathlib.Path(os.environ["FORMULA_PATH"])
tag = os.environ["TAG"]
sha = os.environ["SHA256_VALUE"]
text = formula_path.read_text()

new_text = re.sub(
    r'url "https://github\.com/life2you/lazypost4J/archive/refs/tags/v[^"]+\.tar\.gz"',
    f'url "https://github.com/life2you/lazypost4J/archive/refs/tags/{tag}.tar.gz"',
    text,
    count=1,
)
new_text = re.sub(
    r'sha256 "[0-9a-f]{64}"',
    f'sha256 "{sha}"',
    new_text,
    count=1,
)

if new_text == text:
    print("Formula update failed: no changes applied", file=sys.stderr)
    sys.exit(1)

formula_path.write_text(new_text)
PY

echo "Updated formula: ${FORMULA_PATH}"
echo "Next steps:"
echo "  git -C ${TAP_REPO_PATH} diff -- Formula/lazypost.rb"
echo "  git -C ${TAP_REPO_PATH} commit -am \"Update lazypost to ${TAG}\""
echo "  git -C ${TAP_REPO_PATH} push origin main"

#!/usr/bin/env bash
set -euo pipefail

readonly ROOT="$(
  cd "$(dirname "${BASH_SOURCE[0]}")/../.."
  pwd
)"
readonly LIZARD_VERSION="1.21.2"
export UV_CACHE_DIR="${UV_CACHE_DIR:-${TMPDIR:-/tmp}/apex-exec-uv-cache}"
export UV_TOOL_DIR="${UV_TOOL_DIR:-${TMPDIR:-/tmp}/apex-exec-uv-tools}"
export UV_PYTHON_INSTALL_DIR="${UV_PYTHON_INSTALL_DIR:-${TMPDIR:-/tmp}/apex-exec-uv-python}"

cd "${ROOT}"

actual_version="$(uvx --from "lizard==${LIZARD_VERSION}" lizard --version)"
if [[ "${actual_version}" != "${LIZARD_VERSION}" ]]; then
  echo "expected Lizard ${LIZARD_VERSION}, found ${actual_version}" >&2
  exit 2
fi

uvx --from "lizard==${LIZARD_VERSION}" lizard \
  src \
  --languages rust \
  --CCN 15 \
  --Threshold nloc=80 \
  --csv \
  --ignore_warnings -1 |
  python3 tools/maintainability/check_lizard.py check \
    --baseline tools/maintainability/lizard-baseline.json

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BACKEND_DIR="${BACKEND_DIR:-${REPO_ROOT}/backend}"
FRONTEND_DIR="${FRONTEND_DIR:-${REPO_ROOT}/frontend}"
AGENT_DIR="${AGENT_DIR:-${REPO_ROOT}/agent}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Command '$1' not found. Please install it before continuing." >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd npm
require_cmd python3

echo "==> Building backend (cargo build --release)"
(
  cd "${BACKEND_DIR}"
  cargo build --release
)

echo "==> Installing frontend dependencies (npm install)"
(
  cd "${FRONTEND_DIR}"
  npm install
)

echo "==> Building frontend (npm run build)"
(
  cd "${FRONTEND_DIR}"
  npm run build
)

if [[ ! -d "${AGENT_DIR}" ]]; then
  echo "Agent directory ${AGENT_DIR} not found." >&2
  exit 1
fi

echo "==> Preparing agent virtualenv (${AGENT_DIR}/.venv)"
(
  cd "${AGENT_DIR}"
  if [[ ! -d ".venv" ]]; then
    python3 -m venv .venv
  fi
  # shellcheck source=/dev/null
  source .venv/bin/activate
  pip install --upgrade pip
  pip install -r requirements.txt
  if [[ -f requirements-dev.txt ]]; then
    pip install -r requirements-dev.txt
  fi
)

echo "Build completed successfully."

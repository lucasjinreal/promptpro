#!/usr/bin/env bash
# build_and_publish.sh
# Professional one-shot builder (mac host + docker manylinux):
#  - mac wheels built locally (attempt universal2)
#  - linux manylinux wheels built in Docker (single mac machine)
#  - collects all wheels to OUTPUT_DIR (default ./target)
#  - optional upload to PyPI via PYPI_API_TOKEN
#
# Usage:
#   PYPI_API_TOKEN=... ./build_and_publish.sh
#   ./build_and_publish.sh --no-upload
#
set -euo pipefail
IFS=$'\n\t'

# -------------------------
# Config (tweak as needed)
# -------------------------
OUTPUT_DIR="${OUTPUT_DIR:-./target}"        # final wheel dir
BUILD_TARGET_DIR="${BUILD_TARGET_DIR:-./build}" # cargo target dir
PY_VERSIONS=( "3.8" "3.9" "3.10" "3.11" "3.12" ) # supported Python versions
DOCKER_IMAGE="${DOCKER_IMAGE:-ghcr.io/pyo3/maturin:latest}" # maturin manylinux image
MANYLINUX_TAG="${MANYLINUX_TAG:-2014}"  # manylinux2014
UPLOAD_TO_PYPI=true
TWINE_USERNAME="__token__"

# -------------------------
# Helpers
# -------------------------
log()  { printf '\033[1;34m%s\033[0m\n' ">>> $*"; }
warn() { printf '\033[1;33m%s\033[0m\n' "!!! $*"; }
err()  { printf '\033[1;31m%s\033[0m\n' "XXX $*"; }
die()  { err "$*"; exit 1; }

ensure_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command '$1' not found in PATH"
}

# ensure dirs
mkdir -p "$OUTPUT_DIR"
mkdir -p "$BUILD_TARGET_DIR"

# detect OS
OS="$(uname -s)"
is_mac=false
is_linux=false
case "$OS" in
  Darwin*) is_mac=true ;;
  Linux*)  is_linux=true ;;
  *) die "Unsupported OS: $OS" ;;
esac

# build interpreter args array for maturin
INTERPRETER_ARGS=()
for v in "${PY_VERSIONS[@]}"; do
  INTERPRETER_ARGS+=( "--interpreter" "python${v}" )
done

# -------------------------
# macOS: build locally
# -------------------------
build_mac_local() {
  log "macOS local build: ensure maturin present"
  if ! command -v maturin >/dev/null 2>&1; then
    log "maturin not found -> installing into user python"
    python3 -m pip install --user --upgrade maturin
    export PATH="$HOME/.local/bin:$PATH"
  fi
  ensure_cmd maturin



  # Try universal2 (Intel+Apple Silicon). If it fails, retry without it.
  log "Attempting maturin build with --universal2 (mac universal wheel)"
  set +e
  maturin build --release --target-dir "$BUILD_TARGET_DIR" --out "$OUTPUT_DIR" --universal2 "${INTERPRETER_ARGS[@]}"
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    warn "--universal2 failed or unsupported; retrying without --universal2"
    maturin build --release --target-dir "$BUILD_TARGET_DIR" --out "$OUTPUT_DIR" "${INTERPRETER_ARGS[@]}"
  fi
}

# -------------------------
# Linux: build manylinux in Docker (works on mac host)
# -------------------------
build_linux_manylinux_docker() {
  ensure_cmd docker

  local platform="${DOCKER_PLATFORM:-linux/amd64}"  # default to x86_64 if not set    
#   local platform="${DOCKER_PLATFORM:-linux/arm64}"  # default to x86_64 if not set    
  log "Ensuring Docker image is available: $DOCKER_IMAGE"

  # pull only if missing
  if ! docker image inspect "$DOCKER_IMAGE" >/dev/null 2>&1; then
    log "Pulling Docker image: $DOCKER_IMAGE"
    docker pull "$DOCKER_IMAGE"
  else
    log "Docker image $DOCKER_IMAGE already exists locally, skipping pull"
  fi

  log "Running manylinux build inside Docker (this will create manylinux wheels)"
  docker run --rm --platform "$platform" -v "$(pwd)":/io -w /io "$DOCKER_IMAGE" \
      build \
      --release \
      --target-dir /io/"${BUILD_TARGET_DIR#./}" \
      --out /io/"${OUTPUT_DIR#./}" \
      --manylinux "$MANYLINUX_TAG" \
      "${INTERPRETER_ARGS[@]}"
}

# -------------------------
# helper: collect wheels from likely locations
# -------------------------
collect_wheels() {
  log "Collecting wheels into ${OUTPUT_DIR} (searching common output dirs)"
  mkdir -p "$OUTPUT_DIR"

  candidates=( "${BUILD_TARGET_DIR%/}/wheels" "target/wheels" "./target/wheels" "./wheelhouse" "$OUTPUT_DIR" )

  found=false
  shopt -s nullglob
  for d in "${candidates[@]}"; do
    if [[ -d "$d" ]]; then
      for f in "$d"/*.whl; do
        # copy if not exists; if exists, overwrite to ensure newest
        cp -f "$f" "$OUTPUT_DIR"/
        found=true
      done
    fi
  done
  shopt -u nullglob

  if ! $found; then
    warn "No wheels found. Inspect maturin logs to see where wheels were written."
  else
    log "Collected wheels:"
    ls -1 "$OUTPUT_DIR"/*.whl || true
  fi
}

# -------------------------
# Upload to PyPI
# -------------------------
upload_to_pypi() {
  log "Uploading wheels in ${OUTPUT_DIR} to PyPI using twine (non-interactive)"
  python3 -m pip install --user --upgrade twine
  export PATH="$HOME/.local/bin:$PATH"
  ensure_cmd twine

#   twine upload "${OUTPUT_DIR}"/*.whl -u "$TWINE_USERNAME" -p "$PYPI_API_TOKEN"
  twine upload "${OUTPUT_DIR}"/*.whl -u "$TWINE_USERNAME"
}

# -------------------------
# CLI flags
# -------------------------
NO_UPLOAD=false
while [[ "${1:-}" != "" ]]; do
  case "$1" in
    --no-upload) NO_UPLOAD=true; shift ;;
    --output-dir) OUTPUT_DIR="$2"; mkdir -p "$OUTPUT_DIR"; shift 2 ;;
    --build-dir)  BUILD_TARGET_DIR="$2"; mkdir -p "$BUILD_TARGET_DIR"; shift 2 ;;
    --help|-h) printf "Usage: %s [--no-upload] [--output-dir DIR] [--build-dir DIR]\n" "$0"; exit 0;;
    *) die "Unknown arg: $1";;
  esac
done
if $NO_UPLOAD; then UPLOAD_TO_PYPI=false; fi

# -------------------------
# Main
# -------------------------
log "Starting build process"
log "Python versions: ${PY_VERSIONS[*]}"
log "Output dir: $OUTPUT_DIR, target dir: $BUILD_TARGET_DIR"

# if $is_mac; then
#   build_mac_local
# else
#   log "Not macOS; skipping local mac build"
# fi

# build_linux_manylinux_docker

# collect_wheels

upload_to_pypi

log "Build complete — final wheel list in ${OUTPUT_DIR}:"
ls -1 "${OUTPUT_DIR}"/*.whl || log "No wheels produced."

exit 0

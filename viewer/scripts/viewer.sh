#!/usr/bin/env bash
# Install, run and test the Clio viewers on Linux (and macOS).
#
#   scripts/viewer.sh install [rust|python|all] [--jira]
#   scripts/viewer.sh run rust|python [viewer args...]
#   scripts/viewer.sh test [rust|python|all]
#
# `run` installs first if needed and, without --file or --jira, opens the
# sample plan. Relative paths in viewer arguments are resolved from the directory
# you run the script in. Set CLIO_PYTHON to choose a Python interpreter.
set -euo pipefail

VIEWER_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_DIR="$VIEWER_DIR/rust"
PY_DIR="$VIEWER_DIR/python"
VENV="$PY_DIR/.venv"
RUST_BIN="$RUST_DIR/target/release/clio-viewer"
PY_BIN="$VENV/bin/clio-viewer"
SAMPLE="$VIEWER_DIR/fixtures/sample.json"
# Remembers `install --jira`, so later rebuilds keep the same features.
FEATURES_FILE="$RUST_DIR/target/release/.clio-features"

if [[ -t 2 ]]; then BLUE=$'\033[1;34m' RED=$'\033[1;31m' OFF=$'\033[0m'; else BLUE='' RED='' OFF=''; fi
say() { printf '%s==>%s %s\n' "$BLUE" "$OFF" "$*" >&2; }
die() { printf '%serror:%s %s\n' "$RED" "$OFF" "$*" >&2; exit 1; }

usage() {
    sed -n '2,10p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
    exit "${1:-0}"
}

# --- prerequisites ---------------------------------------------------------

need_cargo() {
    command -v cargo >/dev/null 2>&1 && return
    die "cargo not found. Install Rust from https://rustup.rs (or your distro: e.g. 'sudo dnf install cargo', 'sudo apt install cargo')."
}

# Prints the first Python >= 3.10 found (CLIO_PYTHON wins).
find_python() {
    local c
    for c in ${CLIO_PYTHON:-} python3.14 python3.13 python3.12 python3.11 python3.10 python3 python; do
        command -v "$c" >/dev/null 2>&1 || continue
        if "$c" -c 'import sys; sys.exit(sys.version_info < (3, 10))' 2>/dev/null; then
            command -v "$c"
            return
        fi
    done
    die "Python 3.10+ not found. Install it (e.g. 'sudo dnf install python3', 'sudo apt install python3 python3-venv') or set CLIO_PYTHON."
}

# --- commands --------------------------------------------------------------

build_rust() {  # build_rust [quiet]
    local features=() note=""
    if [[ "${JIRA:-0}" == 1 ]]; then
        features=(--features jira)
        note=", with Jira support"
    fi
    if [[ "${1:-}" == quiet ]]; then
        (cd "$RUST_DIR" && cargo build --release --quiet ${features[@]+"${features[@]}"})
    else
        say "Building the Rust viewer (release$note)"
        (cd "$RUST_DIR" && cargo build --release ${features[@]+"${features[@]}"})
    fi
    if [[ "${JIRA:-0}" == 1 ]]; then echo jira >"$FEATURES_FILE"; else rm -f "$FEATURES_FILE"; fi
}

install_rust() {
    need_cargo
    build_rust
    say "Rust viewer: $RUST_BIN"
}

# Before running: rebuild if the sources changed (a no-op otherwise), with the
# features chosen at install time. Without cargo, run whatever is there.
refresh_rust() {
    if ! command -v cargo >/dev/null 2>&1; then
        [[ -x "$RUST_BIN" ]] || need_cargo
        return
    fi
    if [[ ! -x "$RUST_BIN" ]]; then
        install_rust
        return
    fi
    [[ -f "$FEATURES_FILE" ]] && JIRA=1
    build_rust quiet
}

install_python() {
    local py
    py="$(find_python)"
    if [[ ! -x "$VENV/bin/python" ]]; then
        say "Creating $VENV with $("$py" --version 2>&1)"
        "$py" -m venv "$VENV" || die "could not create a virtual environment (Debian/Ubuntu: 'sudo apt install python3-venv')"
    fi
    say "Installing the Python viewer and its dependencies into the venv"
    "$VENV/bin/python" -m pip install --quiet --upgrade pip
    # PYTHONWARNINGS hides setuptools' editable-install banner.
    PYTHONWARNINGS=ignore "$VENV/bin/python" -m pip install --quiet -e "$PY_DIR[test]"
    say "Python viewer: $PY_BIN"
}

cmd_install() {
    local which=all
    JIRA=0
    for arg in "$@"; do
        case "$arg" in
            rust|python|all) which="$arg" ;;
            --jira) JIRA=1 ;;
            -h|--help) usage ;;
            *) die "install: unknown argument '$arg'" ;;
        esac
    done
    case "$which" in
        rust) install_rust ;;
        python) install_python ;;
        all) install_rust; install_python ;;
    esac
}

cmd_run() {
    local which="${1:-}"
    shift || true
    [[ "${1:-}" == "--" ]] && shift
    local bin
    case "$which" in
        rust) bin="$RUST_BIN"; refresh_rust ;;
        python) bin="$PY_BIN"; [[ -x "$bin" ]] || install_python ;;
        *) die "run: say which viewer: 'run rust' or 'run python'" ;;
    esac
    # No data source given: show the sample plan.
    local arg has_source=0
    for arg in "$@"; do
        case "$arg" in --file|--file=*|--jira) has_source=1 ;; esac
    done
    [[ $has_source == 1 ]] || set -- --file "$SAMPLE" "$@"
    exec "$bin" "$@"
}

cmd_test() {
    local which="${1:-all}"
    case "$which" in
        rust|python|all) ;;
        *) die "test: expected rust, python or all" ;;
    esac
    if [[ "$which" != python ]]; then
        need_cargo
        say "Rust tests"
        (cd "$RUST_DIR" && cargo test)
    fi
    if [[ "$which" != rust ]]; then
        [[ -x "$VENV/bin/python" ]] || install_python
        if command -v cargo >/dev/null 2>&1; then
            # The parity tests compare against the Rust debug build.
            (cd "$RUST_DIR" && cargo build --quiet)
        else
            say "cargo not found: Rust/Python parity tests will be skipped"
        fi
        say "Python tests"
        (cd "$PY_DIR" && "$VENV/bin/python" -m pytest -q)
    fi
}

case "${1:-}" in
    install) shift; cmd_install "$@" ;;
    run) shift; cmd_run "$@" ;;
    test) shift; cmd_test "$@" ;;
    -h|--help|help) usage ;;
    "") usage 1 ;;
    *) die "unknown command '$1' (expected install, run or test)" ;;
esac

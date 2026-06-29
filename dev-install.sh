#!/usr/bin/env bash
#
# cc-uax dev installer — rebuild from source and refresh local skills.
#
# Usage:
#   ./dev-install.sh              build + install, refresh skills
#   ./dev-install.sh uninstall    cargo-uninstall cc-uax and remove local skills
#
# What it does:
#   1. cargo install --path . --force  →  builds and installs `cc-uax` into ~/.cargo/bin
#   2. Copies skills/cc-uax/SKILL.md into Claude Code (~/.claude/skills/cc-uax)
#      and Codex (~/.agents/skills/cc-uax), overwriting any existing copy.
#
# This is a local development helper. For the end-user release installer, see install.sh.
#
set -euo pipefail

# Locate repo root from the script's own location, so it works from any cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

UNINSTALL="${UNINSTALL:-0}"
case "${1:-}" in
    uninstall|--uninstall|-u) UNINSTALL=1 ;;
esac

# ── output helpers ──────────────────────────────────────────────────────────
if [ -t 1 ]; then
    C_BLUE='\033[0;34m'; C_GREEN='\033[0;32m'; C_YELLOW='\033[1;33m'
    C_RED='\033[0;31m'; C_DIM='\033[2m'; C_NC='\033[0m'
else
    C_BLUE=''; C_GREEN=''; C_YELLOW=''; C_RED=''; C_DIM=''; C_NC=''
fi

info() { printf "${C_BLUE}›${C_NC} %s\n" "$*"; }
ok()   { printf "${C_GREEN}✓${C_NC} %s\n" "$*"; }
warn() { printf "${C_YELLOW}!${C_NC} %s\n" "$*"; }
die()  { printf "${C_RED}✗${C_NC} %s\n" "$*" >&2; exit 1; }

# ── uninstall ─────────────────────────────────────────────────────────────────
if [ "$UNINSTALL" = "1" ]; then
    printf "\n${C_BLUE}cc-uax dev uninstall${C_NC}\n"
    removed=0
    if command -v cargo >/dev/null 2>&1; then
        if cargo uninstall cc-uax >/dev/null 2>&1; then
            ok "cargo uninstall cc-uax"
            removed=1
        else
            warn "cc-uax was not installed via cargo"
        fi
    else
        warn "cargo not found — skipping binary removal"
    fi
    for dir in "${HOME}/.claude/skills/cc-uax" "${HOME}/.agents/skills/cc-uax"; do
        if [ -d "$dir" ]; then
            rm -rf "$dir"
            ok "removed ${dir}"
            removed=1
        fi
    done
    if [ "$removed" = "1" ]; then
        printf "\n${C_GREEN}cc-uax dev uninstall complete.${C_NC}\n\n"
    else
        printf "\n${C_YELLOW}nothing to uninstall.${C_NC}\n\n"
    fi
    exit 0
fi

command -v cargo >/dev/null 2>&1 || die "cargo not found on PATH — install Rust first"

CARGO_BIN="${CARGO_HOME:-${HOME}/.cargo}/bin"
SKILL_SRC="${SCRIPT_DIR}/skills/cc-uax/SKILL.md"
[ -f "$SKILL_SRC" ] || die "skill source not found: $SKILL_SRC"

# ── [1/2] build + install binary ─────────────────────────────────────────────
printf "\n${C_BLUE}[1/2]${C_NC} Build and install cc-uax\n"
info "cargo install --path . --force"
cargo install --path . --force
ok "cc-uax → ${CARGO_BIN}/cc-uax"

# ── [2/2] refresh skills (overwrite) ─────────────────────────────────────────
printf "\n${C_BLUE}[2/2]${C_NC} Refresh agent skills\n"
for dest in "${HOME}/.claude/skills/cc-uax" "${HOME}/.agents/skills/cc-uax"; do
    mkdir -p "$dest"
    cp "$SKILL_SRC" "${dest}/SKILL.md"
    ok "skill → ${dest}/SKILL.md"
done

# ── summary ──────────────────────────────────────────────────────────────────
printf "\n${C_GREEN}cc-uax dev install complete.${C_NC}\n"
printf "${C_DIM}Verify:${C_NC} cc-uax --version\n\n"

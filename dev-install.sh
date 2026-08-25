#!/usr/bin/env bash
#
# cc-uax dev installer — rebuild from source and refresh local skills.
#
# Usage:
#   ./dev-install.sh              build + install, link skills into agent homes
#   ./dev-install.sh uninstall    remove the installed binary and skill links
#
# What it does:
#   1. cargo build -p cc-uax-cli --release --locked (incremental)
#   2. Copies target/release/cc-uax into ~/.cargo/bin (override with INSTALL_DIR)
#   3. Symlinks skills/cc-uax into Claude Code (~/.claude/skills/cc-uax),
#      Codex (~/.codex/skills/cc-uax), and legacy Agents (~/.agents/skills/cc-uax)
#
# Environment overrides:
#   INSTALL_DIR        binary install location   (default: $CARGO_HOME/bin)
#   CC_UAX_HOME        skill home root           (default: $HOME / %USERPROFILE%)
#   UNINSTALL=1        remove cc-uax instead of installing
#   KEEP_BOTH=1        if a release copy exists, keep it (no prompt)
#   REPLACE_OTHER=1    if a release copy exists, remove it (no prompt)
#
# This is a local development helper. For the end-user release installer, see install.sh.
#
set -euo pipefail

# Resolve repository paths without changing the caller's working directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_DIR="${SCRIPT_DIR}/crates/cc-uax-cli"
SKILL_SRC="${SCRIPT_DIR}/skills/cc-uax"

is_windows_shell() {
    case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) return 0 ;;
        *) return 1 ;;
    esac
}

# Git Bash inherits Windows HOME=C:\Users\...; backslashes are escape characters
# unless converted. Prefer USERPROFILE via cygpath on those shells.
unix_path() {
    if is_windows_shell && command -v cygpath >/dev/null 2>&1; then
        cygpath -u "$1"
    else
        printf '%s' "$1"
    fi
}

default_home() {
    if [ -n "${CC_UAX_HOME:-}" ]; then
        unix_path "$CC_UAX_HOME"
        return
    fi
    if is_windows_shell && [ -n "${USERPROFILE:-}" ]; then
        unix_path "$USERPROFILE"
        return
    fi
    printf '%s' "$HOME"
}

default_bin_dir() {
    if [ -n "${INSTALL_DIR:-}" ]; then
        unix_path "$INSTALL_DIR"
        return
    fi
    if [ -n "${CARGO_HOME:-}" ]; then
        printf '%s/bin' "$(unix_path "$CARGO_HOME")"
        return
    fi
    printf '%s/.cargo/bin' "$(default_home)"
}

SKILL_HOME="$(default_home)"
BIN_DIR="$(default_bin_dir)"

UNINSTALL="${UNINSTALL:-0}"
KEEP_BOTH="${KEEP_BOTH:-0}"
REPLACE_OTHER="${REPLACE_OTHER:-0}"
case "${1:-}" in
    uninstall|--uninstall|-u) UNINSTALL=1 ;;
    --keep-both) KEEP_BOTH=1 ;;
    --replace-other) REPLACE_OTHER=1 ;;
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

sibling_release_dir() {
    if [ -n "${CC_UAX_RELEASE_DIR:-}" ]; then
        unix_path "$CC_UAX_RELEASE_DIR"
        return
    fi
    if is_windows_shell && [ -n "${LOCALAPPDATA:-}" ]; then
        unix_path "${LOCALAPPDATA}/Programs/cc-uax"
        return
    fi
    printf '%s/.local/bin' "$(default_home)"
}

collect_other_bins() {
    local ours="$1"
    local rel
    rel="$(sibling_release_dir)"
    [ "$rel" != "$ours" ] || return 0
    # Windows treats cc-uax and cc-uax.exe as the same file; list one path.
    if [ -e "${rel}/cc-uax.exe" ]; then
        printf '%s\n' "${rel}/cc-uax.exe"
    elif [ -e "${rel}/cc-uax" ] || [ -L "${rel}/cc-uax" ]; then
        printf '%s\n' "${rel}/cc-uax"
    fi
}

confirm_remove_other() {
    if [ "$REPLACE_OTHER" = "1" ]; then
        return 0
    fi
    if [ "$KEEP_BOTH" = "1" ]; then
        return 1
    fi
    if [ ! -t 0 ]; then
        warn "stdin is not a TTY; keeping both. Re-run with REPLACE_OTHER=1 to remove the other copy."
        return 1
    fi
    printf "Uninstall the other copy? [y/N] "
    ans=""
    read -r ans || ans=""
    case "$ans" in
        y|Y|yes|YES) return 0 ;;
        *) return 1 ;;
    esac
}

invoke_release_uninstall() {
    local rel
    rel="$(sibling_release_dir)"
    # The Windows release installer owns User PATH; call it from Git Bash too.
    if is_windows_shell && [ -f "${SCRIPT_DIR}/install.ps1" ] && command -v powershell.exe >/dev/null 2>&1; then
        local win_script win_dir
        win_script="$(cygpath -w "${SCRIPT_DIR}/install.ps1")"
        win_dir="$(cygpath -w "$rel")"
        MSYS2_ARG_CONV_EXCL='*' powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \
            "\$env:INSTALL_DIR = '${win_dir//\'/\'\'}'; \$env:NO_SKILL = '1'; & '${win_script//\'/\'\'}' -Uninstall"
        return
    fi
    local script="${SCRIPT_DIR}/install.sh"
    if [ ! -f "$script" ]; then
        warn "cannot invoke install.sh (not next to this script); leaving the other copy in place."
        return 1
    fi
    env -u REPLACE_OTHER -u KEEP_BOTH -u UNINSTALL \
        INSTALL_DIR="$rel" \
        NO_SKILL=1 \
        bash "$script" uninstall
}

show_path_winner() {
    if command -v cc-uax >/dev/null 2>&1; then
        printf "${C_DIM}PATH will run:${C_NC} %s\n" "$(command -v cc-uax)"
    fi
}

skill_dests() {
    printf '%s\n' \
        "${SKILL_HOME}/.claude/skills/cc-uax" \
        "${SKILL_HOME}/.codex/skills/cc-uax" \
        "${SKILL_HOME}/.agents/skills/cc-uax"
}

# Remove a skill destination without following a link into the repository tree.
# `rm -rf dest/` (trailing slash) and some MSYS junctions can delete the target.
# Git Bash must not call `cmd /c` without MSYS2_ARG_CONV_EXCL -- `/c` and
# Windows paths get rewritten and mklink/rmdir fail with a syntax error.
remove_skill_dest() {
    local dest="$1"
    if [ -L "$dest" ]; then
        rm -f "$dest"
        return
    fi
    if [ ! -e "$dest" ]; then
        return
    fi
    if is_windows_shell && command -v cygpath >/dev/null 2>&1 && command -v powershell.exe >/dev/null 2>&1; then
        local win
        win="$(cygpath -w "$dest")"
        MSYS2_ARG_CONV_EXCL='*' powershell.exe -NoProfile -Command \
            "\$p = '${win//\'/\'\'}'; if (Test-Path -LiteralPath \$p) { \$i = Get-Item -LiteralPath \$p -Force; if (\$i.Attributes -band [IO.FileAttributes]::ReparsePoint) { [IO.Directory]::Delete(\$p) } else { Remove-Item -LiteralPath \$p -Recurse -Force } }"
        return
    fi
    rm -rf "$dest"
}

link_skill() {
    local dest="$1"
    mkdir -p "$(dirname "$dest")"
    remove_skill_dest "$dest"
    if ln -sfn "$SKILL_SRC" "$dest" 2>/dev/null && [ -L "$dest" ]; then
        return
    fi
    remove_skill_dest "$dest"
    if is_windows_shell && command -v cygpath >/dev/null 2>&1 && command -v powershell.exe >/dev/null 2>&1; then
        local win_dest win_src
        win_dest="$(cygpath -w "$dest")"
        win_src="$(cygpath -w "$SKILL_SRC")"
        MSYS2_ARG_CONV_EXCL='*' powershell.exe -NoProfile -Command \
            "New-Item -ItemType Junction -Path '${win_dest//\'/\'\'}' -Target '${win_src//\'/\'\'}' | Out-Null"
        [ -e "$dest" ] || die "cannot create skill junction: ${dest}"
        return
    fi
    die "cannot link skill to ${dest}"
}

# Honour CARGO_TARGET_DIR (and cargo config) instead of assuming <repo>/target.
cargo_target_dir() {
    local raw
    if [ -n "${CARGO_TARGET_DIR:-}" ]; then
        unix_path "$CARGO_TARGET_DIR"
        return
    fi
    raw="$(cargo metadata --format-version 1 --no-deps --offline --manifest-path "${SCRIPT_DIR}/Cargo.toml" |
        sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
    [ -n "$raw" ] || { printf '%s/target' "$SCRIPT_DIR"; return; }
    raw="${raw//\\\\/\\}"
    unix_path "$raw"
}

built_bin() {
    local td
    td="$(cargo_target_dir)"
    if [ -f "${td}/release/cc-uax.exe" ]; then
        printf '%s' "${td}/release/cc-uax.exe"
    elif [ -f "${td}/release/cc-uax" ]; then
        printf '%s' "${td}/release/cc-uax"
    else
        return 1
    fi
}

bin_name() {
    case "$(built_bin)" in
        *.exe) printf 'cc-uax.exe' ;;
        *)     printf 'cc-uax' ;;
    esac
}

# ── uninstall ─────────────────────────────────────────────────────────────────
if [ "$UNINSTALL" = "1" ]; then
    printf "\n${C_BLUE}cc-uax dev uninstall${C_NC}\n"
    removed=0
    # INSTALL_DIR / CC_UAX_HOME are sandbox overrides -- never cargo-uninstall
    # the caller's real ~/.cargo/bin copy in that case.
    if [ -z "${INSTALL_DIR:-}" ] && [ -z "${CC_UAX_HOME:-}" ] && command -v cargo >/dev/null 2>&1; then
        if cargo uninstall cc-uax-cli >/dev/null 2>&1; then
            ok "cargo uninstall cc-uax-cli"
            removed=1
        fi
    fi
    for name in cc-uax cc-uax.exe; do
        if [ -e "${BIN_DIR}/${name}" ] || [ -L "${BIN_DIR}/${name}" ]; then
            rm -f "${BIN_DIR}/${name}"
            ok "removed ${BIN_DIR}/${name}"
            removed=1
        fi
    done
    while IFS= read -r dir; do
        if [ -e "$dir" ] || [ -L "$dir" ]; then
            remove_skill_dest "$dir"
            ok "removed ${dir}"
            removed=1
        fi
    done < <(skill_dests)
    if [ "$removed" = "1" ]; then
        printf "\n${C_GREEN}cc-uax dev uninstall complete.${C_NC}\n\n"
    else
        printf "\n${C_YELLOW}nothing to uninstall.${C_NC}\n\n"
    fi
    exit 0
fi

command -v cargo >/dev/null 2>&1 || die "cargo not found on PATH — install Rust first"
[ -f "${SKILL_SRC}/SKILL.md" ] || die "skill source not found: ${SKILL_SRC}/SKILL.md"
[ -f "${CLI_DIR}/Cargo.toml" ] || die "CLI package not found: ${CLI_DIR}/Cargo.toml"

other_list="$(collect_other_bins "$BIN_DIR" || true)"
if [ -n "$other_list" ]; then
    printf "\n${C_YELLOW}!${C_NC} Another cc-uax install is present:\n"
    printf '%s\n' "$other_list" | while IFS= read -r p; do
        printf "    %s\n" "$p"
    done
    printf "The release copy is usually earlier on PATH than %s.\n" "$BIN_DIR"
    printf "Keeping both means \`cc-uax\` will still run the release binary.\n"
    printf "Uninstalling the release copy runs the release uninstall script\n"
    printf "(binary and PATH; skills stay and are re-linked to this repository).\n"
    if confirm_remove_other; then
        invoke_release_uninstall || warn "keeping both -- could not run the release uninstall script."
    else
        warn "keeping both -- \`cc-uax\` will still run the release copy."
    fi
fi

# ── [1/2] build + install binary ─────────────────────────────────────────────
printf "\n${C_BLUE}[1/2]${C_NC} Build and install cc-uax\n"
info "cargo build -p cc-uax-cli --release --locked"
cargo build -p cc-uax-cli --release --locked --manifest-path "${SCRIPT_DIR}/Cargo.toml"
src_bin="$(built_bin)" || die "built binary not found under ${SCRIPT_DIR}/target/release"
name="$(bin_name)"
mkdir -p "$BIN_DIR"
cp "$src_bin" "${BIN_DIR}/${name}"
chmod +x "${BIN_DIR}/${name}" 2>/dev/null || true
ok "cc-uax → ${BIN_DIR}/${name}"

# ── [2/2] link skills ────────────────────────────────────────────────────────
printf "\n${C_BLUE}[2/2]${C_NC} Link agent skills\n"
while IFS= read -r dest; do
    link_skill "$dest"
    ok "skill → ${dest}"
done < <(skill_dests)

# ── summary ──────────────────────────────────────────────────────────────────
printf "\n${C_GREEN}cc-uax dev install complete.${C_NC}\n"
show_path_winner
printf "${C_DIM}Verify:${C_NC} cc-uax --version\n\n"

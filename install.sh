#!/usr/bin/env bash
#
# cc-uax one-line installer for Linux / macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
#
# Uninstall (remove the binary and skills):
#   bash install.sh uninstall
#   curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash -s -- uninstall
#
# What it does:
#   1. Detects OS + arch and maps to a release target
#   2. Fetches the latest release version from GitHub
#   3. Downloads the platform archive and verifies its published SHA-256 checksum
#   4. Installs the `cc-uax` binary (default: ~/.local/bin, override with INSTALL_DIR=...)
#   5. Installs the cc-uax skill into Claude Code (~/.claude/skills),
#      Codex (~/.codex/skills), and the legacy Agents path (~/.agents/skills)
#
# Environment overrides:
#   INSTALL_DIR   binary install location        (default: ~/.local/bin)
#   VERSION       specific release tag           (default: latest)
#   NO_SKILL=1    skip skill configuration
#   UNINSTALL=1   remove cc-uax instead of installing
#
set -euo pipefail

REPO="cyber-tao/cc-uax"
INSTALL_DIR="${INSTALL_DIR:-${HOME}/.local/bin}"
NO_SKILL="${NO_SKILL:-0}"
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

info()    { printf "${C_BLUE}›${C_NC} %s\n" "$*"; }
ok()      { printf "${C_GREEN}✓${C_NC} %s\n" "$*"; }
warn()    { printf "${C_YELLOW}!${C_NC} %s\n" "$*"; }
err()     { printf "${C_RED}✗${C_NC} %s\n" "$*" >&2; }
die()     { err "$*"; exit 1; }
step()    { printf "\n${C_BLUE}[%s/%s]${C_NC} %s\n" "$1" "$TOTAL_STEPS" "$2"; }
TOTAL_STEPS=5

# ── uninstall ───────────────────────────────────────────────────────────────
if [ "$UNINSTALL" = "1" ]; then
    printf "\n${C_BLUE}cc-uax uninstall${C_NC}\n"
    removed=0
    BIN="${INSTALL_DIR}/cc-uax"
    if [ -e "$BIN" ]; then
        rm -f "$BIN"
        ok "removed ${BIN}"
        removed=1
        # Drop the install dir only if it is now empty (never touch a shared bin dir).
        rmdir "$INSTALL_DIR" 2>/dev/null && ok "removed empty ${INSTALL_DIR}" || true
    else
        warn "binary not found: ${BIN}"
    fi

    if [ "$NO_SKILL" = "1" ]; then
        warn "NO_SKILL=1 — leaving skills in place"
    else
        for dir in "${HOME}/.claude/skills/cc-uax" "${HOME}/.codex/skills/cc-uax" "${HOME}/.agents/skills/cc-uax"; do
            if [ -d "$dir" ]; then
                rm -rf "$dir"
                ok "removed ${dir}"
                removed=1
            fi
        done
    fi

    if [ "$removed" = "1" ]; then
        printf "\n${C_GREEN}cc-uax uninstalled.${C_NC}\n\n"
    else
        printf "\n${C_YELLOW}nothing to uninstall.${C_NC}\n\n"
    fi
    exit 0
fi

# ── prerequisites ───────────────────────────────────────────────────────────
need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}
need_cmd curl
need_cmd tar
need_cmd awk
need_cmd sed
need_cmd tr

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "required command not found: sha256sum or shasum"
    fi
}

TMPDIR="$(mktemp -d 2>/dev/null || die "cannot create temp dir")"
trap 'rm -rf "$TMPDIR"' EXIT

# ── [1/5] detect platform ───────────────────────────────────────────────────
step 1 "Detecting platform"
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux)
        case "$arch" in
            x86_64|amd64)  TARGET="x86_64-unknown-linux-gnu" ;;
            aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
            *) die "unsupported Linux arch: $arch" ;;
        esac ;;
    Darwin)
        case "$arch" in
            x86_64|amd64)  TARGET="x86_64-apple-darwin" ;;
            aarch64|arm64) TARGET="aarch64-apple-darwin" ;;
            *) die "unsupported macOS arch: $arch" ;;
        esac ;;
    *) die "unsupported OS: $os — Windows users should run install.ps1 in PowerShell" ;;
esac
ok "OS=$os  arch=$arch  →  target=$TARGET"

# ── [2/5] resolve version ───────────────────────────────────────────────────
step 2 "Resolving latest version"
if [ -n "${VERSION:-}" ]; then
    case "$VERSION" in
        v*) TAG="$VERSION" ;;
        *)  TAG="v${VERSION}" ;;
    esac
else
    api_url="https://api.github.com/repos/${REPO}/releases/latest"
    release_json="$(curl -fsSL "$api_url")" || die "cannot fetch latest release metadata"
    TAG="$(printf '%s\n' "$release_json" | sed -n 's/^[[:space:]]*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
    [ -n "$TAG" ] || die "cannot resolve latest release (network error or rate limited)"
fi
VERSION_NUM="${TAG#v}"
ok "version=${VERSION_NUM} (${TAG})"

# ── [3/5] download ──────────────────────────────────────────────────────────
step 3 "Downloading"
ARCHIVE="cc-uax-${TARGET}-${VERSION_NUM}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
CHECKSUM_URL="https://github.com/${REPO}/releases/download/${TAG}/SHA256SUMS"
info "$URL"
curl -fL --progress-bar -o "${TMPDIR}/${ARCHIVE}" "$URL" || die "download failed"
curl -fsSL -o "${TMPDIR}/SHA256SUMS" "$CHECKSUM_URL" || die "checksum download failed"
expected_sha="$(awk -v name="$ARCHIVE" '$2 == name || $2 == "*" name { print $1; exit }' "${TMPDIR}/SHA256SUMS")"
[ "${#expected_sha}" -eq 64 ] || die "invalid checksum for ${ARCHIVE}"
case "$expected_sha" in
    *[!0-9A-Fa-f]*) die "invalid checksum for ${ARCHIVE}" ;;
esac
expected_sha="$(printf '%s' "$expected_sha" | tr 'A-F' 'a-f')"
actual_sha="$(sha256_file "${TMPDIR}/${ARCHIVE}")"
[ "$actual_sha" = "$expected_sha" ] || die "checksum mismatch for ${ARCHIVE}"
# validate it's actually a gzip archive, not a 404 HTML page
tar -tzf "${TMPDIR}/${ARCHIVE}" >/dev/null 2>&1 || die "archive is corrupt or target asset missing: $ARCHIVE"
ok "downloaded and verified ${ARCHIVE}"

# ── [4/5] install binary ────────────────────────────────────────────────────
step 4 "Installing binary"
STAGE="cc-uax-${TARGET}-${VERSION_NUM}"
tar -xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"
[ -f "${TMPDIR}/${STAGE}/cc-uax" ] || die "cc-uax binary not found in archive"
[ -f "${TMPDIR}/${STAGE}/LICENSE" ] || die "LICENSE missing in archive"

mkdir -p "$INSTALL_DIR"
install -m 0755 "${TMPDIR}/${STAGE}/cc-uax" "${INSTALL_DIR}/cc-uax"
ok "binary → ${INSTALL_DIR}/cc-uax"

case ":${PATH}:" in
    *":${INSTALL_DIR}:"*) ;;
    *)
        warn "${INSTALL_DIR} is not on your PATH."
        printf "  Add this to your shell profile (~/.bashrc / ~/.zshrc):\n"
        printf "    ${C_DIM}export PATH=\"%s:\$PATH\"${C_NC}\n" "$INSTALL_DIR"
        ;;
esac

# ── [5/5] configure skills ──────────────────────────────────────────────────
step 5 "Configuring agent skills"
if [ "$NO_SKILL" = "1" ]; then
    warn "NO_SKILL=1 set — skipping skill configuration"
else
    SKILL_SRC="${TMPDIR}/${STAGE}/skills/cc-uax"
    [ -f "${SKILL_SRC}/SKILL.md" ] || die "SKILL.md missing in archive"

    for dest in "${HOME}/.claude/skills/cc-uax" "${HOME}/.codex/skills/cc-uax" "${HOME}/.agents/skills/cc-uax"; do
        rm -rf "$dest"
        mkdir -p "$dest"
        cp -R "${SKILL_SRC}/." "$dest/"
        ok "skill → ${dest}"
    done
fi

# ── summary ─────────────────────────────────────────────────────────────────
printf "\n${C_GREEN}cc-uax ${VERSION_NUM} installed.${C_NC}\n"
if command -v cc-uax >/dev/null 2>&1; then
    printf "${C_DIM}Verify:${C_NC} cc-uax --version\n"
else
    printf "${C_DIM}Open a new shell, then:${C_NC} cc-uax --version\n"
fi
printf "\n"

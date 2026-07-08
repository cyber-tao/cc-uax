#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONTENT_DIR="${CC_UAX_CONTENT_DIR:-D:/WorkDir/ClashOfPets/Content}"
UE_SOURCE_DIR="${CC_UAX_UE_SOURCE_DIR:-E:/UnrealEngine_5.7}"
EXE="${CC_UAX_EXE:-${REPO_ROOT}/target/release/cc-uax}"
LIMIT="${1:-${CC_UAX_VALIDATE_LIMIT:-0}}"

windows_path_to_wsl() {
    local path="$1"
    if [[ "$path" =~ ^([A-Za-z]):/(.*)$ ]]; then
        local drive="${BASH_REMATCH[1],,}"
        local rest="${BASH_REMATCH[2]}"
        printf '/mnt/%s/%s' "$drive" "$rest"
    else
        printf '%s' "$path"
    fi
}

if [ ! -d "$CONTENT_DIR" ]; then
    alt_content="$(windows_path_to_wsl "$CONTENT_DIR")"
    if [ -d "$alt_content" ]; then
        CONTENT_DIR="$alt_content"
    fi
fi
if [ ! -d "$UE_SOURCE_DIR" ]; then
    alt_source="$(windows_path_to_wsl "$UE_SOURCE_DIR")"
    if [ -d "$alt_source" ]; then
        UE_SOURCE_DIR="$alt_source"
    fi
fi
if [ ! -x "$EXE" ] && [ -x "${EXE}.exe" ]; then
    EXE="${EXE}.exe"
fi

if [ ! -x "$EXE" ]; then
    (cd "$REPO_ROOT" && cargo build --release)
fi
if [ ! -x "$EXE" ] && [ -x "${EXE}.exe" ]; then
    EXE="${EXE}.exe"
fi

if [ ! -x "$EXE" ]; then
    printf 'cc-uax executable not found: %s\n' "$EXE" >&2
    exit 1
fi
if [ ! -d "$CONTENT_DIR" ]; then
    printf 'content directory not found: %s\n' "$CONTENT_DIR" >&2
    exit 1
fi

for rel in \
    Engine/Source/Runtime/CoreUObject/Private/UObject/PropertyTag.cpp \
    Engine/Source/Runtime/Engine/Private/EdGraph/EdGraphPin.cpp
do
    if [ ! -f "${UE_SOURCE_DIR}/${rel}" ]; then
        printf 'warning: UE source reference missing: %s\n' "${UE_SOURCE_DIR}/${rel}" >&2
    fi
done

mapfile -t files < <(find "$CONTENT_DIR" -type f \( -iname '*.uasset' -o -iname '*.umap' \) | sort)
if [ "$LIMIT" -gt 0 ] 2>/dev/null; then
    files=("${files[@]:0:${LIMIT}}")
fi
if [ "${#files[@]}" -eq 0 ]; then
    printf 'no .uasset/.umap files found under %s\n' "$CONTENT_DIR" >&2
    exit 1
fi

to_exe_path() {
    local path="$1"
    if [[ "$EXE" == *.exe ]] && command -v wslpath >/dev/null 2>&1; then
        wslpath -w "$path"
    else
        printf '%s' "$path"
    fi
}

run_section() {
    local section="$1"
    local failed=0
    local diagnostics=0
    local unparsed=0
    local i=0
    local out
    local exe_file
    for file in "${files[@]}"; do
        i=$((i + 1))
        if [ "$i" -eq 1 ] || [ $((i % 100)) -eq 0 ] || [ "$i" -eq "${#files[@]}" ]; then
            printf '[%s] %s/%s %s\n' "$section" "$i" "${#files[@]}" "$file"
        fi
        exe_file="$(to_exe_path "$file")"
        if ! out="$("$EXE" -S "$section" --compact "$exe_file" 2>&1)"; then
            failed=$((failed + 1))
            printf '%s\n' "$out" >&2
            continue
        fi
        if [[ "$out" != *'"diagnostics":[]'* ]]; then
            diagnostics=$((diagnostics + 1))
        fi
        if [[ "$out" == *'"@unparsed"'* ]]; then
            unparsed=$((unparsed + 1))
        fi
    done
    printf '%s: total=%s failed=%s diagnostic_files=%s unparsed_files=%s\n' \
        "$section" "${#files[@]}" "$failed" "$diagnostics" "$unparsed"
    if [ "$failed" -ne 0 ] || [ "$diagnostics" -ne 0 ] || [ "$unparsed" -ne 0 ]; then
        return 1
    fi
}

run_section debug
run_section all

sample="${files[0]}"
refs_out="$("$EXE" -S refs --scan-dir "$(to_exe_path "$CONTENT_DIR")" --no-cache --compact "$(to_exe_path "$sample")" 2>&1)"
printf 'reverse reference sample: %s\n' "$sample"
printf '%s\n' "$refs_out" | tail -n 1

printf 'Real asset validation passed.\n'

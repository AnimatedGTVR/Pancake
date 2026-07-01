#!/usr/bin/env bash
set -euo pipefail

shopt -s nullglob

isos=(iso/out/*.iso iso/out/debian/*.iso iso/out/arch/*.iso)

if ((${#isos[@]} == 0)); then
    echo "No ISO files found under iso/out."
    exit 0
fi

for iso in "${isos[@]}"; do
    label="$(isoinfo -d -i "$iso" 2>/dev/null | awk -F': ' '/Volume id:/ {print $2; exit}')"
    case "$label" in
        PANCAKE_DEBIAN) base="Debian Live" ;;
        PANCAKE_*) base="legacy Arch/archiso" ;;
        "") base="unknown" ;;
        *) base="unknown ($label)" ;;
    esac

    printf '%-48s %s\n' "$iso" "$base"
done

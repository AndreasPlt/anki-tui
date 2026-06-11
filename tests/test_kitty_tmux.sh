#!/bin/bash
# Test Kitty graphics with tmux DCS passthrough wrapping.
# Run this INSIDE a tmux pane/popup to test passthrough.
# Usage: bash tests/test_kitty_tmux.sh [image_path]

IMG="${1:-$(find ~/Library/Application\ Support/Anki2/User\ 1/collection.media/ -name '*.png' | head -1)}"

if [ ! -f "$IMG" ]; then
    echo "No image found. Pass an image path as argument."
    exit 1
fi

echo "Testing image: $IMG"
echo "TMUX=$TMUX"
echo "allow-passthrough: $(tmux show-options -g allow-passthrough 2>/dev/null)"
echo ""

ENCODED_DATA=$(base64 < "$IMG")
KITTY_SEQ=$(printf "\033_Ga=T,f=100,t=d;%s\033\\" "$ENCODED_DATA")

# Test 1: Raw (no wrapping) — should NOT work in tmux
echo "--- Test 1: raw Kitty escape (no DCS wrap) ---"
printf "%s" "$KITTY_SEQ"
echo ""
echo "(expected: NO image in tmux)"
echo ""

# Test 2: Single DCS passthrough wrap — should work in tmux
echo "--- Test 2: single DCS passthrough wrap ---"
# Wrap: \ePtmux; + (inner with doubled ESCs) + \e\\
WRAPPED=$(printf "%s" "$KITTY_SEQ" | sed 's/\x1b/\x1b\x1b/g')
printf "\033Ptmux;%s\033\\" "$WRAPPED"
echo ""
echo "(expected: image visible if allow-passthrough works)"
echo ""

# Test 3: Using printf to build the wrap more reliably
echo "--- Test 3: manual DCS wrap with printf ---"
printf "\033Ptmux;\033\033_Ga=T,f=100,t=d;\033\\"
# For small images, send entire payload in one chunk
printf "\033Ptmux;\033\033_Ga=T,f=100,t=d,m=0;%s\033\033\134\033\134" "$ENCODED_DATA"
echo ""
echo "(expected: image visible)"

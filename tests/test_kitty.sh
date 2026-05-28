#!/bin/bash
# Test Kitty graphics protocol in your terminal
# Usage: bash tests/test_kitty.sh [image_path]

IMG="${1:-$(find ~/Library/Application\ Support/Anki2/User\ 1/collection.media/ -name '*.png' | head -1)}"

if [ ! -f "$IMG" ]; then
    echo "No image found. Pass an image path as argument."
    exit 1
fi

echo "Testing image: $IMG"
echo ""

# Test 1: t=f (file path mode)
echo "--- Test 1: file path mode (t=f) ---"
ENCODED_PATH=$(echo -n "$IMG" | base64)
printf "\033_Ga=T,t=f;%s\033\\" "$ENCODED_PATH"
echo ""
echo "(should see image above)"
echo ""

# Test 2: t=d (direct data mode) with PNG
echo "--- Test 2: direct data mode (t=d, f=100) ---"
ENCODED_DATA=$(base64 < "$IMG")
printf "\033_Ga=T,f=100,t=d;%s\033\\" "$ENCODED_DATA"
echo ""
echo "(should see image above)"

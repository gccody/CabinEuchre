#!/bin/bash
# Renders the App Store screenshots into store/screenshots/, from the game's own mobile layouts.
# Run from the repo root with `cd server && npx wrangler dev` going, for the lobby. Needs Pillow.
# The sizes are what App Store Connect asks for: 6.9" iPhone and 13" iPad, landscape.
set -u
EXE=target/debug/cabin-euchre.exe
cargo build || exit 1
cargo test --test scene_checks --no-run || exit 1

shoot() { # device, window, safe area
  local out="store/screenshots/$1" win=$2 safe=$3
  rm -rf "$out"
  mkdir -p "$out"
  export EUCHRE_DEFAULT_SETTINGS=1 EUCHRE_FIXED_DT=1 EUCHRE_MOBILE=1 EUCHRE_WINDOW="$win" EUCHRE_SAFE="$safe"
  shot() { # name, frame, switches
    local name=$1 frame=$2
    shift 2
    env LOVE2DRS_SCREENSHOT="$out/$name.png" LOVE2DRS_SCREENSHOT_FRAME="$frame" "$@" "$EXE" >/dev/null 2>&1
  }
  shot 1-bidding 340 EUCHRE_SCENE=table EUCHRE_SEED=15 EUCHRE_AUTOPLAY=1
  shot 2-trick 1420 EUCHRE_SCENE=table EUCHRE_SEED=15 EUCHRE_AUTOPLAY=1
  shot 4-lobby 240 EUCHRE_SCENE=host EUCHRE_SEED=15
  shot 5-title 150
  shot 6-trophies 700 EUCHRE_SCENE=trophies EUCHRE_RECORD=demo
  # The lesson: a picture of each beat comes from walking it in a real window.
  local proof
  proof=$(mktemp -d)
  env -u EUCHRE_FIXED_DT EUCHRE_PROOF_SHOTS="$proof" EUCHRE_PROOF_SPEED=3 \
    cargo test --test scene_checks -- tutorial_plays_through >/dev/null 2>&1
  cp "$proof/009_task.png" "$out/3-learn.png"
  rm -rf "$proof"
  # The store refuses screenshots with an alpha channel.
  python - "$out" "$win" <<'EOF'
import glob, sys
from PIL import Image
want = tuple(int(n) for n in sys.argv[2].split("x"))
files = sorted(glob.glob(sys.argv[1] + "/*.png"))
assert len(files) == 6, files
for f in files:
    img = Image.open(f).convert("RGB")
    assert img.size == want, (f, img.size)
    img.save(f, optimize=True)
print(sys.argv[1], len(files), want)
EOF
}

shoot iphone-6.9 2868x1320 186,0,186,63
shoot ipad-13 2752x2064 0,0,0,40

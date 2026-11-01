# Euchre

A Euchre card game in Rust on the [love2drs](https://github.com/gccody/Love2DRS) framework, with
a cozy Great Lakes cabin presentation: chunky pixel art on a lamp-lit walnut table with a felt
mat, brass-trimmed walnut panels, a paper score pad, and springy feedback with punchy sounds
(lively, but deliberately not over the top).

**Status: complete and playable.** Title screen, full rules (bidding rounds, bowers, going alone,
stick the dealer, scoring to 5/7/10), heuristic AI for the three other seats, results and
game-over screens, pause menu, options (persisted) and an illustrated How to Play. Plays with
mouse, keyboard, touchscreen or gamepad.

Use the `love2drs` skill (`.claude/skills/love2drs/`) for framework API questions. Its
`references/api.md` is the API reference. Don't guess method names.

## Commands

```sh
cargo run                      # debug build (deps are optimized, so it runs smoothly)
cargo run --release
cargo test                     # rules + AI, including 400 simulated AI-only games
cargo clippy --all-targets     # keep it warning-free
LOVE2DRS_SCREENSHOT=target/shots/x.png LOVE2DRS_SCREENSHOT_FRAME=60 cargo run   # render one frame to PNG and exit
```

After any visual change, capture a screenshot and look at it. Scenes animate, so try a couple
of frame numbers. In game, F2 toggles the lamplight post-process (useful for judging raw art; it restores the
slider's strength when turned back on) and F11 toggles fullscreen.

### Debug switches (environment variables)

Frame numbers only map to game time with `EUCHRE_FIXED_DT`, because the real frame rate follows
the monitor's refresh rate.

| variable | effect |
|---|---|
| `EUCHRE_FIXED_DT=n` | every frame advances exactly n/60 s (1 for screenshots, 4 to fast-forward soak runs) |
| `EUCHRE_SCENE=table\|options\|rules` | start in a game, or on the title with a panel open |
| `EUCHRE_SEED=n` | deterministic deal, AI think times and shuffles |
| `EUCHRE_AUTOPLAY=1` | the AI also plays your seat (your prompt shows for 1.2 s first); results and game over auto-continue, and game overs are logged to stderr |
| `EUCHRE_TARGET=n` | override points to win (1 reaches the game-over screen quickly) |
| `EUCHRE_INPUT=pad\|touch` | start as if that device was used last (screenshots of the gamepad focus highlight and pad hints without a controller) |
| `EUCHRE_DEFAULT_SETTINGS=1` | ignore (and never write) the saved settings file, so screenshots don't depend on the player's options |

Example: `EUCHRE_FIXED_DT=1 EUCHRE_SCENE=table EUCHRE_SEED=15 EUCHRE_AUTOPLAY=1 LOVE2DRS_SCREENSHOT=target/shots/bid.png LOVE2DRS_SCREENSHOT_FRAME=340 cargo run`
shows your order-up prompt. Seed 15 has you bid in both rounds, 24 has you discard as dealer,
45 has your partner go alone.

## Architecture

| file | role |
|---|---|
| `src/main.rs` | window config, `love2drs::run` |
| `src/app.rs` | the `Game` impl: owns `Renderer`, `Shared` and the active `Scene`, applies settings each frame, global hotkeys, scene fade, input routing (touch, gamepad, d-pad/stick auto-repeat, rumble) |
| `src/scene.rs` | `Scene` trait, `Shared` resources (font, card art, sfx, settings, time, design-space pointer, input mode), `Nav`, `Transition` |
| `src/euchre/` | **pure rules, no love2drs imports**: `card` (suits, ranks, bower-aware `power`), `round` (one hand's state machine), `game` (`Match` to N points), `ai` (heuristics), `rng` |
| `src/scenes/title.rs` | logo, a fan of the four jacks (click to flip), main menu |
| `src/scenes/table/` | the game: `mod.rs` state, cue timeline, actions and input; `layout.rs` where every place sits; `draw.rs` all rendering |
| `src/scenes/options.rs`, `rules.rs` | modal overlays shared by title and table (`scenes::Overlay`) |
| `src/render.rs` | 640x360 design space rendered into a canvas at the window's own resolution, tabletop background, lamplight present pass, `View::to_design`. `DENSITY` (1.5) is the pixel art's texels per design pixel |
| `src/shaders/*.wgsl` | `background.wgsl` (walnut table, felt mat, swaying lamp), `pixel.wgsl` (anti-aliased pixel-art sampling for every scene draw) and `lamplight.wgsl` (post-process). Loaded with `include_str!` |
| `src/text.rs` | hand-authored 5x7 bitmap pixel font (`GLYPHS`) and the `Text` draw builder. One font pixel is `UNIT` (4/3) design px at scale 1 |
| `src/cards.rs` | procedural pixel-art card atlas (faces, court portraits, back) and white suit icons; `Pose`. Painted at `DENSITY`, drawn at `1/DENSITY`. Atlases are linear-filtered and every quad has a 1-texel transparent border (`cards::bordered`) so `pixel.wgsl` can smooth sprite outlines |
| `src/suits.rs` | suit shapes: math masks at any size, plus hand-drawn 11 and 13 px masks so small clubs and spades stay distinct |
| `src/ui.rs` | `Rect`, panels, springy `Button` |
| `src/palette.rs` | every color the game uses |
| `src/juice.rs` | `Spring`, easing curves, `wobble` |
| `src/sfx.rs` | procedural sound effects (`Sound` enum), pitch jitter, synthesized lounge music loop |
| `src/settings.rs` | options persisted to `%APPDATA%\Euchre\settings.txt` (or `~/.config/euchre/`) |
| `src/assets.rs` | asset path lookup (cwd, crate root, exe dir) |

### How the table works

- `Match`/`Round` in `src/euchre` are the source of truth. The scene only calls
  `Round::apply(seat, action)` and `Round::advance()`.
- Every card has a `Place` (pile, up card, hand, trick, won pile, kitty, sitting out). `layout.rs`
  turns places into target positions each frame and per-card springs chase them, so cards never
  snap. Change a card's place with `Table::move_card`, which also bumps its draw order.
- Pacing goes through a queue of `Cue`s (`Wait`, `Deal`, `AiAct`, `CollectTrick`, ...). When the
  queue is empty, `drive()` looks at the round's phase and schedules the next step, or sets up
  the human's prompt and waits for input.
- `perform()` applies an action and mirrors it visually (bubbles, trump toast, card moves).

### Input

- `Shared::input` is the device used last: `Mouse`, `Touch` or `Buttons` (gamepad or arrow
  keys). `shared.mouse` is the mouse, the finger while one is down, or `scene::NO_POINTER`, so
  hover effects just work and vanish on a gamepad.
- Gamepad and arrow keys arrive as `Scene::nav(Nav)`: d-pad/left stick (auto-repeat), A
  `Confirm`, B `Back`, Start/Back `Menu`, Y `Alt` (alone), LB/RB pages. Once the focus is
  showing, Enter and Space are `Confirm` too. Each scene keeps a focus index and sets
  `Button::focused` / `Slider::focused`; focus only draws when `shared.focus_visible()`.
- Touch acts on release (`Scene::touch_released`, defaulting to `mouse_pressed`), so you can
  slide along your hand before lifting to play. Sliders start dragging in `touch_pressed`.
  The app ignores mouse events for a moment after any touch: the OS emulates them.
- Rumble: `shared.rumble(strength, seconds)`. It plays only on a gamepad with the Rumble option
  on, after the player last pressed a controller button (mouse motion alone does not stop it).
  Save it for the same big moments as screen shake. A press that actually selects something
  (menu item, option value, bid, card) calls `shared.tap()`; moves and refused presses do not.

### Rules of the road

- **Design space.** Scenes draw in a fixed 640x360 coordinate space and receive mouse positions
  already converted to it. Never use window size or `ctx.mouse` coordinates directly in a
  scene. Use `shared.mouse` or the `pos` argument.
- **Colors** come from `palette.rs`. Add new named constants there instead of inline hex values.
- **UI text** goes through `text::Text` (hard drop shadow in `palette::INK` by default; ♥♦ tint
  red automatically). Add missing characters to `GLYPHS` in `text.rs`.
- **Motion** should never snap. Drive positions, scale and rotation with `juice::Spring` or the
  easing functions. Kick a spring (`impulse`) for feedback on every interaction. Keep it
  tasteful: small overshoot, short shakes only for euchres, marches and game over.
- **Every control is reachable without a mouse.** New buttons need a place in their scene's
  focus order and a `nav` path to them.
- **Sound**: every interaction makes a sound. Add variants to `sfx::Sound` and play them via
  `shared.sfx.play(...)`, which already adds pitch jitter. Use `play_pitched` for rising pitch
  on streaks (each trick won).
- **Game logic stays out of rendering.** Rules, AI and scoring live in `src/euchre` with unit
  tests. Scenes only display state and forward input.

## Art direction

A Great Lakes cabin card night: euchre as it's played on a kitchen or lodge table. The game was
once Balatro-inspired and was deliberately redesigned to have its own identity, so avoid drifting
back toward Balatro's look: no swirling painted backgrounds, CRT scanlines or barrel curvature,
slate-gray rounded panels, rainbow button rows or a run-info sidebar. Never copy Balatro's (or any
other game's) sprites, shaders, sounds or other files.

- Chunky pixel art, rendered at the window's full resolution: texels stay square and crisp, but
  rotated and scaled sprites have smooth, anti-aliased edges (`pixel.wgsl`). No stair-stepping.
- Table (`background.wgsl`): walnut planks with staggered joints under a lake-teal felt mat with
  a stitched edge, snapped to the pixel-art texel grid. A pendant lamp sways very slowly; its
  light falls off in ordered-dither (Bayer) bands, and a few dust motes drift through the beam.
  Colors are the `WOOD*`, `FELT*` and `LAMP` palette constants.
- Lamplight pass (`lamplight.wgsl`): sharp-bilinear upscale, warm grade, oval vignette, fine film
  grain at 12 fps. Strength is the "Atmosphere" slider in Options (saved as `atmosphere`; the
  old `crt` key is still read).
- UI: chamfered (cut-corner) shapes via `ui::chamfer`, thick dark outlines, brass trim. Panels
  are walnut with a brass rim, inner hairline and corner studs. Buttons are beveled plaques over a
  darker lip. The primary action is brass with engraved dark lettering (`Button::brass`); others
  are pine, lake blue, cranberry or walnut.
- Table HUD: no sidebar. The score is a tilted paper score pad (top left: WE / THEY, pencil
  numbers and tally marks, trick dots, a note), trump is a brass medallion with a ribbon (top
  right). The pad stays lit above the result banner's dim so the points visibly tick up.
- Cards feel physical: hover lift and tilt toward the mouse, flips with a little hop, springy
  deals and plays, a shadow under every card, a gold pulse on the trick winner. Backs are buffalo
  check plaid with a pine-tree badge.
- Font: the custom bitmap font in `text.rs`. Cards are 67x91 texels (about 45x61 design px) and painted in `cards.rs`.
- Sound: short, punchy, slightly randomized. Music is synthesized unless
  `assets/music/theme.ogg` exists.

## Ideas for later

- Smarter AI (card counting for voids, better lone-hand judgment), difficulty levels.
- Optional house rules: defending alone, "farmer's hand", no-trump.
- Statistics (games won, euchres) saved alongside settings.

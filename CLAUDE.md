# Euchre

A Euchre card game in Rust on the [love2drs](https://github.com/gccody/Love2DRS) framework, with
a Balatro-inspired presentation: chunky pixels, a swirling painted background, CRT glow, and
springy feedback with punchy sounds (lively, but deliberately not over the top).

**Status: complete and playable.** Title screen, full rules (bidding rounds, bowers, going alone,
stick the dealer, scoring to 5/7/10), heuristic AI for the three other seats, results and
game-over screens, pause menu, options (persisted) and an illustrated How to Play.

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
of frame numbers. In game, F2 toggles the CRT effect (useful for judging raw art; it restores the
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
| `EUCHRE_DEFAULT_SETTINGS=1` | ignore (and never write) the saved settings file, so screenshots don't depend on the player's options |

Example: `EUCHRE_FIXED_DT=1 EUCHRE_SCENE=table EUCHRE_SEED=15 EUCHRE_AUTOPLAY=1 LOVE2DRS_SCREENSHOT=target/shots/bid.png LOVE2DRS_SCREENSHOT_FRAME=340 cargo run`
shows your order-up prompt. Seed 15 has you bid in both rounds, 24 has you discard as dealer,
45 has your partner go alone.

## Architecture

| file | role |
|---|---|
| `src/main.rs` | window config, `love2drs::run` |
| `src/app.rs` | the `Game` impl: owns `Renderer`, `Shared` and the active `Scene`, applies settings each frame, global hotkeys, scene fade |
| `src/scene.rs` | `Scene` trait, `Shared` resources (font, card art, sfx, settings, time, design-space mouse), `Transition` |
| `src/euchre/` | **pure rules, no love2drs imports**: `card` (suits, ranks, bower-aware `power`), `round` (one hand's state machine), `game` (`Match` to N points), `ai` (heuristics), `rng` |
| `src/scenes/title.rs` | logo, a fan of the four jacks (click to flip), main menu |
| `src/scenes/table/` | the game: `mod.rs` state, cue timeline, actions and input; `layout.rs` where every place sits; `draw.rs` all rendering |
| `src/scenes/options.rs`, `rules.rs` | modal overlays shared by title and table (`scenes::Overlay`) |
| `src/render.rs` | 640x360 design space rendered at `DENSITY` 1.5 into a 960x540 canvas, `snap`, swirl background, CRT present pass, `View::to_design` |
| `src/shaders/*.wgsl` | `background.wgsl` (swirl) and `crt.wgsl` (post-process). Loaded with `include_str!` |
| `src/text.rs` | hand-authored 5x7 bitmap pixel font (`GLYPHS`) and the `Text` draw builder. One font pixel is 2 canvas px (`UNIT` = 4/3 design px); use text scales in steps of 0.5 so pixels stay whole |
| `src/cards.rs` | procedural pixel-art card atlas (faces, court portraits, back) and white suit icons; `Pose`. Painted at canvas density, drawn at `1/DENSITY` |
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
- **Sound**: every interaction makes a sound. Add variants to `sfx::Sound` and play them via
  `shared.sfx.play(...)`, which already adds pitch jitter. Use `play_pitched` for rising pitch
  on streaks (each trick won).
- **Game logic stays out of rendering.** Rules, AI and scoring live in `src/euchre` with unit
  tests. Scenes only display state and forward input.

## Art direction

The *feel* of Balatro, built from original assets and code. Never copy Balatro's
sprites, shaders, sounds or other files.

- Low-res canvas scaled up with nearest filtering. Pixel-art sprites at 1x canvas scale.
- Background: slow, hypnotic swirl of 3 palette colors (felt greens by default). Tune in
  `background.wgsl` and the `FELT*` palette constants. `Renderer::swirl` controls the twist.
- CRT pass: sharp-bilinear upscale, gentle barrel curvature, scanlines, chromatic aberration,
  vignette. Strength is the "CRT strength" slider in Options (0 to 100%).
- Cards feel physical: hover lift and tilt toward the mouse, flips with a little hop, springy
  deals and plays, a shadow under every card, a gold pulse on the trick winner.
- Bold, saturated accents on dark panels. Thick dark outlines and drop shadows.
- Font: the custom bitmap font in `text.rs`. Cards are 67x91 canvas px (about 45x61 design px) and painted in `cards.rs`.
- Sound: short, punchy, slightly randomized. Music is synthesized unless
  `assets/music/theme.ogg` exists.

## Ideas for later

- Smarter AI (card counting for voids, better lone-hand judgment), difficulty levels.
- Optional house rules: defending alone, "farmer's hand", no-trump.
- Statistics (games won, euchres) saved alongside settings.

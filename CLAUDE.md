# Euchre

A Euchre card game in Rust on the [love2drs](https://github.com/gccody/Love2DRS) framework, with
a cozy Great Lakes cabin presentation: chunky pixel art on a lamp-lit walnut table with a felt
mat, brass-trimmed walnut panels, a paper score pad, and springy feedback with punchy sounds
(lively, but deliberately not over the top).

**Status: complete and playable.** Title screen, full rules (bidding rounds, bowers, going alone,
stick the dealer, scoring to 5/7/10), heuristic AI for the three other seats, results and
game-over screens, pause menu, save & quit with CONTINUE on the title, options (persisted) and
an illustrated How to Play. Plays with mouse, keyboard, touchscreen or gamepad. Runs on Windows,
macOS and Linux, and on Android and iOS phones and tablets (see "Phones and tablets").

**Online play** (cross-platform): host a table or join one with a 5-letter code; bots fill empty
seats. Rooms run on a Cloudflare Worker with one Durable Object per table (`server/`). Steam builds
(`--features steam`) add "Join Game" from the friends list and an invite button. Tables are private:
"Join Game" only asks the host (LET IN / NO) unless the host made the table public, while an
invite or the code sits you straight down. Steam only passes codes and room handles around, so
Steam and phone players share tables.

Use the `love2drs` skill (`.claude/skills/love2drs/`) for framework API questions. Its
`references/api.md` is the API reference. Don't guess method names.

## Commands

```sh
cargo run                      # debug build (deps are optimized, so it runs smoothly)
cargo run --release
cargo test                     # rules + AI + online rooms (simulated players, drops, resyncs)
cargo clippy --all-targets     # keep it warning-free (also with --features steam)
cd server && npx wrangler dev  # the room server on ws://127.0.0.1:8787 (see server/README.md)
cargo run -p euchre-core --example netcheck -- ws://127.0.0.1:8787 3   # play a game against it
LOVE2DRS_SCREENSHOT=target/shots/x.png LOVE2DRS_SCREENSHOT_FRAME=60 cargo run   # render one frame to PNG and exit
cargo love2drs android run -p euchre --abi x86_64   # build, install and start on an emulator (needs cargo-love2drs)
cargo love2drs ios build -p euchre                  # on a Mac: builds the .app for the simulator
EUCHRE_MOBILE=1 EUCHRE_WINDOW=1280x590 EUCHRE_SAFE=50,0,50,24 cargo run   # preview the phone layouts on the desktop
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
| `EUCHRE_SCENE=table\|continue\|options\|rules` | start in a game, resume the saved game, or on the title with a panel open |
| `EUCHRE_SCENE=online\|host\|code` | the PLAY ONLINE menu, hosting a new table right away, or the code keyboard (with "KQJ" typed). With any `EUCHRE_SCENE` (or autoplay) the lobby logs the table code and handle when you sit down, for `netcheck` |
| `EUCHRE_SERVER=ws://...` | the room server (also read at build time; default `ws://127.0.0.1:8787`, i.e. `wrangler dev`) |
| `EUCHRE_SEED=n` | deterministic deal, AI think times and shuffles |
| `EUCHRE_AUTOPLAY=1` | the AI also plays your seat (your prompt shows for 1.2 s first); results and game over auto-continue, and game overs are logged to stderr. In the lobby it deals 4 s after sitting down |
| `EUCHRE_TARGET=n` | override points to win (1 reaches the game-over screen quickly) |
| `EUCHRE_INPUT=pad\|touch` | start as if that device was used last (screenshots of the gamepad focus highlight and pad hints without a controller) |
| `EUCHRE_DEFAULT_SETTINGS=1` | ignore (and never write) the settings file, the saved game and the remembered online seat, so screenshots don't depend on the player's options or a game they left |
| `EUCHRE_MOBILE=1` | use the phone and tablet layouts on the desktop (read from the environment only) |
| `EUCHRE_WINDOW=WxH` | start with this window size, for previewing other screen shapes |
| `EUCHRE_SAFE=l,t,r,b` | with `EUCHRE_MOBILE`, pretend the OS's bars and cutouts take this much of each edge (window units) |

These only work in debug builds, except `EUCHRE_SERVER`, which any build reads. Nothing can set
environment variables on a phone, so debug builds there also read `KEY=value` lines
from `euchre-debug.txt` in the save directory (Android: `files/`, via `adb shell run-as`; iOS: the app
container's `Documents/`). The lobby's `[euchre] seated at table` line goes to logcat on Android.

Example: `EUCHRE_FIXED_DT=1 EUCHRE_SCENE=table EUCHRE_SEED=15 EUCHRE_AUTOPLAY=1 LOVE2DRS_SCREENSHOT=target/shots/bid.png LOVE2DRS_SCREENSHOT_FRAME=340 cargo run`
shows your order-up prompt. Seed 15 has you bid in both rounds, 24 has you discard as dealer,
45 has your partner go alone.

## Architecture

| file | role |
|---|---|
| `src/main.rs` | window config, `love2drs::run` |
| `src/app.rs` | the `Game` impl: owns `Renderer`, `Shared` and the active `Scene`, applies settings each frame, global hotkeys, scene fade, input routing (touch, gamepad, d-pad/stick auto-repeat, rumble) |
| `src/device.rs` | `mobile()`, which every phone and tablet branch goes through (always false on a desktop unless previewing), and `var()` for debug switches |
| `src/scene.rs` | `Scene` trait, `Shared` resources (font, card art, sfx, settings, time, design-space pointer, input mode), `Nav`, `Transition` |
| `core/` | the `euchre-core` crate (imported as `crate::euchre`). **Pure rules, no love2drs or I/O**, so it also builds for the wasm server: `card` (suits, ranks, bower-aware `power`), `round` (one hand's state machine), `game` (`Match` to N points, logs each hand's steps), `save` (text snapshot of a match, replayed on load), `ai` (heuristics), `rng`, and online play: `protocol` (JSON messages), `room` (four seats around the real match), `mirror` (a player's view with unseen cards guessed). `online_tests.rs` plays whole games between a room and simulated players |
| `server/` | the room server: a Rust Cloudflare Worker, one Durable Object per room code (its own cargo workspace, built for wasm by `worker-build`). See `server/README.md` |
| `src/net/` | `Link`: where the table's moves go. `local.rs` runs a `Room` in-process with three bots (offline play); `online.rs` is the WebSocket client on a background thread (rustls, reconnects with its seat token); `seat_memory.rs` keeps that token on disk, so REJOIN works after the game was killed |
| `src/platform/` | Steam behind the `steam` feature (`steam.rs`): player name, rich presence `connect` (`+knock HANDLE`), the invite dialog (`+connect CODE`), join requests (command line or the overlay). No-ops without it |
| `src/scenes/lobby.rs` | PLAY ONLINE: host or join, an on-screen keyboard for codes and names, the seated view (code tiles, private/public, seats, LET IN / NO, DEAL, INVITE, LEAVE), waiting for the host after a knock. Hands the link to the table once the room deals |
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
| `src/savegame.rs` | the save slot, `savegame.txt` in the same folder: read, write (atomically), forget a finished game |
| `src/assets.rs` | asset path lookup (cwd, crate root, exe dir) |

### How the table works

- **Every game runs in a `Room`** (`core/src/room.rs`), offline or online. The room holds the real
  `Match`, plays the bots instantly, and sends each player messages written from their point of
  view (they're always seat 0, their team is team 0). The table never sees the real match: it
  keeps a `Mirror`, which has your exact hand and everything public, while the other hands and the
  kitty are guesses. When a seat plays a card the mirror had guessed elsewhere, the two hidden
  cards trade places (`Mirror::reveal` returns the swaps, and `Table::swap_cards` swaps their
  sprites; both are face down, so nothing visibly changes).
- Your moves go through `Mirror::act` (the same check the room makes), then `link.send(Act)`.
  Other seats' moves arrive as `ServerMsg::Acted` and wait in `pending` until `drive()` is idle;
  bots get a "thinking" `Wait` first, people don't. A deal arrives as `Deal`; a reconnect or a
  resync as `Sync` (the whole hand), laid out like a resumed save. Anything that doesn't fit the
  mirror triggers `request_resync()`.
- Offline: `net::LocalTable` wraps a room with you plus three bots. `Table::new` and
  `Table::resume` build one; saving writes the room's `Match`.
- Every card has a `Place` (pile, up card, hand, trick, won pile, kitty, sitting out). `layout.rs`
  turns places into target positions each frame and per-card springs chase them, so cards never
  snap. Change a card's place with `Table::move_card`, which also bumps its draw order.
- Pacing goes through a queue of `Cue`s (`Wait`, `Deal`, `Remote`, `CollectTrick`, ...). When the
  queue is empty, `drive()` looks at the mirror's phase and schedules the next step, takes the
  next message from the room, or sets up your prompt and waits for input.
- `perform()` (your move) and `perform_remote()` (from the room) both end in `show_action()`,
  which mirrors the move visually (bubbles, trump toast, card moves).
- Saving: a save is the RNG state at the start of the current hand plus every step since.
  Loading re-deals and replays through the rules, so a bad file is rejected, never half-loaded.
  "SAVE & QUIT" (pause menu) and closing the window mid-game write it (`Scene::suspend`); game
  over deletes it, but only if it holds that game (`savegame::forget(id)`). `Table::resume`
  works out every card's place from the round (`settled_places`) and deals them back out. To
  screenshot a resume, point `APPDATA` at a scratch folder holding `Euchre\savegame.txt` and use
  `EUCHRE_SCENE=continue` (screenshot mode exits without `Game::quit`, so it never saves).

### Online play

- Protocol, room rules and hidden information live in `core/` and are covered by
  `online_tests.rs`. Never send a player another seat's cards: the room's `Shown::of` hides the
  dealer's discard from everyone but the dealer.
- Changing a message's shape means bumping `protocol::PROTOCOL_VERSION`.
- **Private by default.** The room code is the invitation: `/join/CODE` (typed, or a Steam invite
  `+connect CODE`) sits you down. Steam rich presence only ever holds the room's *handle* (its
  Durable Object id) as `+knock HANDLE`, and `/knock/HANDLE` waits for the host (the first person
  seated; `Room::pass_host` moves the job on) to answer, unless the host made the table public.
  Never put the code in rich presence or anywhere friends can see it. The host answers in the
  lobby (a LET IN / NO card over the felt) or, mid-game, in the pause menu (MENU shows the count).
- **A connection holds a seat's token, never a seat number** (`Attachment` in `server/src/lib.rs`).
  The room says where a token sits (`Room::seat_of_token`, `Room::token_at`), so a player's
  connection follows them through SIT HERE, and one left open after `Leave` holds nothing.
- **The host's job stays with someone connected** (`Room::check_host`): it moves on when the host's
  connection drops. With nobody connected, waiting requests are turned away, and an empty private
  table refuses new ones rather than handing itself to whoever knocks first.
- The room server (`server/src/lib.rs`) only moves messages, saves the room after every change
  (it may hibernate at any moment) and keeps one alarm for turn timeouts and room expiry. Keep it
  timer-free otherwise: bots move instantly there, and the games add the pauses. A number passed to
  `set_alarm` is a delay from now, not a time of day.
- Timing lives in `core/src/room.rs`, as pure code over times the server passes in. `TurnClock`: 60 s
  a turn, 15 s from a drop, at least 15 s after coming back, and never past 120 s. A person away for
  10 minutes (`AWAY_SEAT_MS`) loses the seat to a bot. A timed-out player sees "Out of time!".
- New connections pass a rate limit set to 60 a minute per address (`[[ratelimits]]` in
  `server/wrangler.toml`, `allowed` in the server). Cloudflare counts approximately, so live it only
  stops heavy bursts (about 190 a minute got through), and `wrangler dev` is exact, so test it live. Requests to join at
  one table are capped at `MAX_KNOCKS`.
- Closing the game on purpose sends `Leave`. Being killed doesn't, so the seat stays for a while,
  and PLAY ONLINE offers REJOIN TABLE. Typing the same code again finds the same seat too.
- Online tables differ only a little: the result banner moves on by itself after 7 s, PLAY AGAIN
  asks the room for a new game, the pause menu says LEAVE TABLE (a bot takes the seat) and adds
  INVITE FRIENDS on Steam, and the table code shows above MENU.
- Steam: `cargo run --features steam` needs `steam_api64.dll` (copied by `steamworks-sys` into
  `target/<profile>/build/steamworks-sys-*/out/`) next to the exe and a `steam_appid.txt`
  holding `480` (Valve's test app) in the working directory while developing. The `#AtTable`
  rich presence token is in `steam/rich_presence_english.vdf`; upload it on the app's Steamworks
  page. The feature only builds for Windows, macOS and Linux; mobile builds use codes.

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
  The app ignores mouse events for a moment after any touch: the OS emulates them. A touch the OS
  takes away (back gesture, call, losing focus) arrives as `touch_cancelled`, not a release, so it
  never taps. Scenes don't hear about it: the pointer just goes away, which ends a slider drag.
- Rumble: `shared.rumble(strength, seconds)`. It plays only on a gamepad with the Rumble option
  on, after the player last pressed a controller button (mouse motion alone does not stop it).
  Save it for the same big moments as screen shake. Smaller pulses go through `shared.buzz(delay,
  low, high, seconds)` (low = heavy motor, high = light motor; the app schedules delayed pulses
  and merges overlaps): a press that actually selects something (menu item, option value, bid,
  card) calls `shared.tap()` (a crisp high-motor click), a focus move or slider step that
  actually moves calls `shared.tick()`, and the deal uses `riffle()`, `gather_slide()` and
  `deal_flick()`. Refused presses don't rumble.

### Phones and tablets

- **Same game, own layouts.** Every mobile difference sits behind `device::mobile()`, which is false on
  a desktop, so the PC game doesn't change. Packaging is `[package.metadata.love2drs]` in `Cargo.toml`.
- **Check the PC build after touching shared code.** Render the same deterministic screenshots
  (`EUCHRE_DEFAULT_SETTINGS=1 EUCHRE_FIXED_DT=1 EUCHRE_SEED=n`) from before and after and compare them
  pixel by pixel. It catches what the eye won't: `background.wgsl` keeps literal numbers for the exact
  640x360 view, because the mat's edges land exactly on their thresholds and computing them from a
  uniform moved the right-hand stitches by a texel.
- **View.** On mobile the 640x360 design space is scaled to fit the safe area, and the canvas covers the
  whole screen, so the tabletop runs full-bleed behind the notch and the bars. `render::bounds()` is the
  safe area in design coordinates. It can reach outside 640x360, so edge-anchored things (hand, MENU,
  score pad, trump medallion) use it. Anything drawn across the whole screen, like `ui::dim_screen`, has
  to cover more than 640x360 too.
- **Margins only grow** while the window keeps its size, and Android starts with a 24 dp bottom floor:
  its gesture bar appears after the first touch, and the table must not jump when it does.
- **Touch targets** are about 40 design px tall plus `ui::TOUCH_SLOP`. One design px is roughly one dp.
- **Lifecycle.** `Scene::background` runs when the app is sent away: offline games save, and online games
  keep their seat (the room's disconnect timer applies). Android's back gesture arrives as `Key::Escape`;
  it pauses in a game, closes panels and leaves from the title. iOS has no back, and never quits itself.
- **No quit or fullscreen on mobile:** the title has no QUIT button and Options has no Fullscreen row.
- **Shipping builds need `EUCHRE_SERVER` set at build time.** The default is `ws://127.0.0.1:8787`, and
  a phone has no room server on localhost.
- **Store builds:** `cargo love2drs android package -p euchre --keystore upload.jks --key-alias upload
  --bundletool bundletool-all.jar` makes an `.aab` (password in `LOVE2DRS_KEYSTORE_PASS`), and on a Mac
  `cargo love2drs ios package -p euchre --identity NAME --profile PATH` makes an `.ipa` and `.dSYM`.
  The iOS privacy manifest is generated and declares no data collection. If the App Store answers need
  to say otherwise (online play sends a name and an address), write one and set `privacy_manifest`.
- **Sound** is left to the framework: it pauses in the background and after an interruption, and it comes back
  by itself. On iOS it mixes with the player's music and obeys the silent switch (`AudioSession::Ambient`).
- **Graphics:** Android uses Vulkan where there's a driver and OpenGL ES 3 otherwise. To test GLES, start an
  emulator with `-gpu swiftshader_indirect -feature -Vulkan`. There, `textureDimensions` returns 0 (the GLSL
  is correct, so this looks like the emulator's translator; not yet seen on a real device), so `pixel.wgsl` falls
  back to plain filtering and sprites look soft. Compare against Vulkan before blaming a change for blurry art.
- Emulator notes: to reach a local `wrangler dev`, use `adb reverse tcp:8787 tcp:8787`.

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

- Public quick-match (an extra Durable Object as a queue), once enough people play online.
- Verified identities (Steam auth tickets checked by the server) if rankings ever matter.

- Smarter AI (card counting for voids, better lone-hand judgment), difficulty levels.
- Optional house rules: defending alone, "farmer's hand", no-trump.
- Statistics (games won, euchres) saved alongside settings.

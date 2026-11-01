# Euchre

The classic four-player trick-taking card game with a juicy, CRT-glow, Balatro-inspired
presentation, built in Rust on [love2drs](https://github.com/gccody/Love2DRS).

You and your partner (Benny, across the table) play against Rosa and Jules. Pick trump, take
tricks, euchre the other team, first to 10 wins.

## Run

```sh
cargo run --release      # or plain `cargo run`; dependencies are optimized either way
```

Requires Rust 1.88+.

## How to play

The in-game **How to Play** screen covers the rules. In short: only 9 through A are used; the jack
of trump (right bower) is the best card and the other jack of the same color (left bower) is
second best and counts as trump. Players take turns ordering up the turned card or naming trump,
then must follow suit when they can. Makers need 3 of 5 tricks: 1 point, a march (all 5) is 2,
a lone march is 4, and getting euchred gives the other team 2.

### Controls

| input | action |
|---|---|
| click a card, or **1-6** | play (or discard) that card |
| bid buttons, or **O** / **P** / **A** / **C D S H** | order up / pass / alone / name a suit |
| **Space** / **Enter** | continue after a hand |
| **Esc** | pause menu (resume, options, how to play, main menu) |
| **F2** | toggle the CRT effect (set its strength with the slider in Options) |
| **F11** | toggle fullscreen |

**Options:** volume, music, CRT strength, game speed (relaxed / normal / fast), points to win
(5 / 7 / 10), stick the dealer, fullscreen. Settings are saved to `%APPDATA%\Euchre\settings.txt`
(or `~/.config/euchre/settings.txt`).

## Assets

Everything on screen and in the speakers is generated in code: the pixel font, the card faces and
backs, every sound effect and the music loop. Optional override:

| path | effect |
|---|---|
| `assets/music/theme.ogg` | replaces the synthesized background music |

## Tests

```sh
cargo test        # rules, bowers, follow-suit edge cases, scoring, and 400 simulated AI games
cargo clippy      # kept warning-free
```

## Developing against a local love2drs

Uncomment the `[patch]` section in `Cargo.toml` to use a local checkout of the framework instead
of the GitHub version.

See [CLAUDE.md](CLAUDE.md) for architecture, conventions, debug switches and the art direction.

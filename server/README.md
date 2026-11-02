# Euchre room server

A Cloudflare Worker (Rust, compiled to wasm) with one **Durable Object per table**. Every online
game, from Steam, phone or anything else, plays at one of these rooms, so everyone can sit at the
same table. Steam only passes room codes around (see `src/platform/steam.rs` in the game).

The rules aren't here. `euchre-core` (`../core`) has them: `room.rs` holds the real match and
answers every message with per-player messages; `protocol.rs` is the wire format; `mirror.rs` is
what each player's game keeps. This crate only moves messages between sockets, saves the room after
every change, and runs one alarm per room for turn timeouts and clean-up.

## Endpoints

| route | what |
|---|---|
| `GET /create?target=10&stick=1` | WebSocket: open a new room (random 5-letter code) with those rules |
| `GET /join/{CODE}` | WebSocket: join or rejoin a room |
| `GET /` | a plain-text hello |

After connecting, the game sends `Hello { version, name, token }`. A token from an earlier
`Welcome` gets the same seat back. Everything else is in `core/src/protocol.rs`.

## Run it locally

Needs Rust with the `wasm32-unknown-unknown` target, `worker-build`
(`cargo install worker-build`) and Node (for `npx wrangler`).

```sh
cd server
npx wrangler dev            # builds with worker-build, serves ws://127.0.0.1:8787
```

The game connects to `ws://127.0.0.1:8787` by default. In another terminal:

```sh
# Plays a whole 3-player game (plus a bot) over real sockets, with a mid-game reconnect.
cargo run -p euchre-core --example netcheck -- ws://127.0.0.1:8787 3

# Or host from the game, and let two simulated players join its table.
EUCHRE_SCENE=host EUCHRE_AUTOPLAY=1 cargo run      # prints "seated at table ABCDE"
cargo run -p euchre-core --example netcheck -- ws://127.0.0.1:8787 2 ABCDE
```

## Deploy

```sh
cd server
npx wrangler login
npx wrangler deploy          # prints https://euchre.<your-subdomain>.workers.dev
```

Then build the game against it (the address is baked in at build time; `EUCHRE_SERVER` at run time
overrides it):

```sh
EUCHRE_SERVER=wss://euchre.<your-subdomain>.workers.dev cargo build --release
```

A custom domain (`wss://play.yourgame.com`) can be added under the Worker's *Settings > Domains*.

## Cost

The Durable Objects are SQLite-backed, which the **Workers Free plan** allows (daily limits
apply; when one is hit, requests fail until 00:00 UTC). Past that, the $5/month Workers Paid plan
covers a great deal. What keeps it cheap:

- **WebSocket hibernation.** An idle room is evicted from memory and isn't billed for duration.
  It's reloaded from storage when the next message arrives.
- **Bots move instantly** on the server. The players' games add the "thinking" pauses while they
  animate, so a room is awake for milliseconds per message.
- **No timers while waiting.** The only alarm is the turn timer (45 s, or 15 s for a player whose
  connection dropped) and room expiry (1 hour after everyone leaves, 24 hours without a message).
- A game to 10 is roughly 300 small messages and a few hundred storage writes.

## Notes

- Changing the message format means bumping `PROTOCOL_VERSION` in `core/src/protocol.rs`. Rooms
  refuse other versions with a polite "please update" error.
- Changing `RoomSnapshot` breaks rooms that are live during a deploy. They only last an hour or so
  anyway, but prefer adding `#[serde(default)]` fields.
- Room codes use 24 letters (no I or O), so there are about 8 million. Joining a code that isn't a
  live room gets `NoSuchRoom` over the socket. For extra protection, a [rate limiting
  binding](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/) on
  `/join` would slow down anyone guessing codes.

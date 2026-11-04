# Trophies and stats

The game calls achievements **trophies**. The rules and the list live in `core/src/record.rs`, and every
platform keeps a full local copy in `stats.txt`, next to the settings. The stores are a second copy:

| build | where trophies also go | code |
|---|---|---|
| Steam (`--features steam`) | Steam achievements | `src/platform/steam.rs` |
| iOS | Game Center | `src/platform/game_center.rs` |
| Android | Google Play Games | `src/platform/play_games.rs` |
| everything else | nowhere else | |

**Stats are local only.** They aren't sent to any store, so they don't follow a player to a new device.

Trophies sync both ways. Everything earned locally is reported at every launch, which catches anything
earned offline or signed out, and the stores ignore what they already have. Whatever the store says the player
has comes back into `stats.txt` without a banner, so a reinstall or a new phone gets them back.

## The list

Ids never change: they're saved in files and set up on the three stores. Each store needs every trophy
set up before it will accept it, using the names and descriptions below.

| id | name | how |
|---|---|---|
| `first_game` | PULL UP A CHAIR | Finish your first game. |
| `first_win` | CABIN CHAMP | Win a game. |
| `tutorial` | BENNY'S STAR PUPIL | Finish Learn to Play. |
| `euchre` | EUCHRED 'EM | Euchre the other team. |
| `march` | MARCHING ORDERS | Take all 5 tricks after your team calls. |
| `lone_made` | FLYING SOLO | Go alone and make it. |
| `lone_march` | LONE LOON | Go alone and take all 5 tricks. |
| `stop_lone` | NOT SO FAST | Euchre someone who went alone. |
| `partner_ride` | ALONG FOR THE RIDE | Your partner goes alone and marches. |
| `stuck` | STUCK AND MADE IT | Get stuck as dealer and make it. |
| `both_bowers` | BOTH BARRELS | Play both bowers in a hand you score. |
| `all_suits` | HOUSE OF CARDS | Call each of the four suits as trump. |
| `shutout` | SKUNKED | Win before they score a point. |
| `comeback` | COMEBACK TRAIL | Win after trailing by 5 or more. |
| `clean` | CLEAN SHEET | Win a game without being euchred. |
| `streak_3` | HOT HAND | Win 3 games in a row. |
| `streak_5` | ON A TEAR | Win 5 games in a row. |
| `wins_10` | TABLE REGULAR | Win 10 games. |
| `wins_50` | LODGE LEGEND | Win 50 games. |
| `euchre_25` | THORN IN THEIR SIDE | Euchre the other team 25 times. |
| `hands_100` | LATE NIGHT | Play 100 hands. |
| `hands_1000` | LAKE SEASON | Play 1,000 hands. |
| `online_win` | LONG DISTANCE | Win a game online. |
| `full_table` | FULL TABLE | Finish an online game with 4 people. |

`core/src/record.rs` has a test that this table can't drift from: see `the_docs_list_every_trophy`.

## Setting up the stores

The code is in place, but nothing reaches a store until these are done. Until then each build quietly keeps
trophies locally. All three also need an icon for each trophy, which hasn't been drawn yet.

### Steam

1. Steamworks > your app > Stats & Achievements > Achievements: add each one with its **id as the API name**.
2. Publish the changes. Steam refuses achievements it doesn't know, and the game retries every 5 seconds.

To test without a real app id, `steam_appid.txt` holding `480` runs against Spacewar, which has its own
achievements, so these report as unknown there.

### Game Center

1. Certificates, Identifiers & Profiles: turn on **Game Center** for `com.gccody.cabineuchre`, then make a new
   provisioning profile. `cargo love2drs` signs with the entitlements the profile lists, so that's the whole
   of the build side.
2. App Store Connect > your app > Game Center > Achievements: add each one with the id
   **`com.gccody.cabineuchre.` + its id**, like `com.gccody.cabineuchre.first_win`.

Game Center needs a real signed build to test. A simulator build has no entitlement, so GameKit
says so in the log and the game carries on with local trophies.

### Google Play Games

Play Games is the one store that needs turning on in this repo, because its SDK makes the app bigger and any
Android build then needs Gradle.

1. Play Console > your app > Grow users > Play Games Services > Setup and management: create the project, link
   the app's signing key, and add every achievement. Google makes up each one's id.
2. Get resources, and copy the ids into `IDS` in `src/platform/play_games.rs`. Trophies not listed there stay local.
3. Turn it on in `Cargo.toml`, with the project id from the same page:

   ```toml
   [package.metadata.love2drs]
   android_dependencies = ["com.google.android.gms:play-services-games-v2:22.1.0"]
   android_meta_data = { "com.google.android.gms.games.APP_ID" = "YOUR_PROJECT_ID" }
   ```

4. Build with a `cargo-love2drs` that has `android_dependencies` (see "The packager" below).

Testers have to be listed on the Play Games Services page, and sign-in only works with a build signed by a key
linked in step 1. The SDK signs players in by itself at launch. Anyone who declined gets a button on the first
page of the trophy shelf.

### The packager

Play Games is Java and resources in Maven packages, and `cargo love2drs` used to package no Java at all. It now takes
`android_dependencies` and has Gradle make the base APK for games that list any, so other games aren't affected. It
needs Gradle on `PATH`, in `GRADLE`, or left behind by another project's wrapper. The first build downloads the
Android Gradle Plugin, and later ones reuse the result until the settings or icon change.

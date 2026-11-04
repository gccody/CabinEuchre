# App Store listing (iOS)

What goes into App Store Connect. Don't mention other platforms or stores here: guideline 2.3.10 doesn't allow it, so no
Steam or Android. This repo is public, so the review contact's phone and email stay out of it.

## App information

| field | value |
|---|---|
| Name (30) | Cabin Euchre |
| Subtitle (30) | Classic euchre by lamplight |
| Bundle ID | com.gccody.cabineuchre |
| Primary category | Games: Card |
| Secondary subcategory | Games: Family |
| Copyright | © 2026 Gccody |
| Content Rights | No third-party content (`theme.ogg` was made by the developer) |
| Privacy Policy URL | https://gccody.github.io/CabinEuchre/privacy.html |
| Support URL | https://gccody.github.io/CabinEuchre/support.html |
| Marketing URL | leave empty |
| Support email | cabineuchresupport@mail.gccody.com |
| Version | 1.0.0 |
| Price | $1.99 |
| Availability | All countries and regions |
| Release | Manually, after approval |

## Promotional text (170)

Pull up a chair. The lamp's on, the cards are out, and Benny needs a partner. Learn to play with him, then host a table for your friends.

## Keywords (100)

card,cards,trick,trump,bowers,partners,bots,multiplayer,offline,classic,michigan,ohio,canada,learn

## Description (4000)

Euchre belongs on a kitchen table at the lake, with the wood stove going and someone keeping score on the back of an envelope. Cabin Euchre brings that table to you: worn walnut, a felt mat, a lamp swaying overhead and a pencil score pad with real tally marks.

Every card has weight. They hop when they flip and snap down on the felt with a good, punchy sound. It's lively, it's cozy, and it's very hard to play just one more hand.

THE REAL RULES
• Right and left bowers, ordering up, naming trump and all the rest
• Go alone when your hand is too good to share
• Stick the dealer, on or off
• Play to 5, 7 or 10, and watch the points tick up on the score pad

NEVER PLAYED? BENNY WILL TEACH YOU
Learn to Play is a real match against Rosa and Jules, with your partner Benny talking you through it. The first hand he points at every move. By the last one he keeps quiet and just watches, to see if you've got it. There's a hint if you get stuck, and a pop quiz or two. Prefer to read? The illustrated How to Play covers it all.

BRING YOUR FRIENDS TO THE TABLE
Host a table, share the 6-letter code, and everyone sits right down. Tables are private unless you say otherwise, and bots keep any empty seats warm, so two of you can take on the house or fill all four chairs.

A FULL TABLE IN YOUR POCKET
Phones and tablets get their own layouts, with big buttons for your thumbs and a table that runs edge to edge.

PLAY IT YOUR WAY
• No connection needed to play against the bots
• Save and quit any time, then pick up right where you left off
• 24 trophies to earn, synced with Game Center, and your lifetime record
• Relaxed, normal and fast game speeds
• Touch or game controller, with rumble on controllers

The lamp's on. Deal me in.

## App Review information

| field | value |
|---|---|
| Sign-in required | No |
| Contact | Entered directly in App Store Connect, not kept here |

Notes:

No account or sign-in is needed. Game Center is only used for achievements, and the game plays the same without it.

To try online play on one device: PLAY ONLINE, then HOST A TABLE, then DEAL. Bots fill the empty seats. A second device can join with the 6-letter code shown on the table.

There is no chat. The only thing players write is a name of up to 10 letters and digits, shown to people at the same table and to the host of a table they ask to join. Tables are private by default, reached with the code.

The game has no ads, no in-app purchases and no wagering.

## App Privacy (a draft; you confirm these)

Data collected, with the reason for each answer. The questions are in App Store Connect under App Privacy.

| question | answer | why |
|---|---|---|
| Do you collect data? | Yes | Online play sends the name you type to the room server, which keeps it while the table exists |
| Type | User Content: Other User Content | A nickname, not a real name: it needn't be one, and it's limited to 10 characters |
| Purpose | App Functionality | It shows who's sitting where |
| Linked to the user? | No | No accounts. The seat token is random and only works at that table |
| Used for tracking? | No | No ads, analytics or third-party SDKs |

`ios/PrivacyInfo.xcprivacy` says the same. `cargo love2drs` only generates a manifest when there isn't one, so a new
required-reason API has to be added to that file by hand.

## Age rating (a draft; you confirm these)

Answer "None" or "No" to everything about violence, adult themes, drugs, gambling and contests: euchre here has no
money, wagers or prizes. Unrestricted web access: No. Messaging and chat: No. If the form asks about user-generated
content, the nickname is the only case, and the review notes above explain it.

## Export compliance

The app uses encryption, only the standard TLS for its connection to the room server (rustls), so it qualifies for the
exemption and needs no documents.

## Trophies

The values for each are in `docs/achievements.md`, and the images in `store/trophies/` come from
`tools/make_trophies.py`.

Points (920 of 1000):

| id | points | id | points | id | points |
|---|---|---|---|---|---|
| first_game | 10 | stuck | 30 | wins_10 | 40 |
| first_win | 10 | both_bowers | 20 | wins_50 | 90 |
| tutorial | 10 | all_suits | 30 | euchre_25 | 50 |
| euchre | 20 | shutout | 50 | hands_100 | 20 |
| march | 20 | comeback | 60 | hands_1000 | 70 |
| lone_made | 30 | clean | 30 | online_win | 40 |
| lone_march | 60 | streak_3 | 40 | full_table | 40 |
| stop_lone | 40 | streak_5 | 70 | | |
| partner_ride | 40 | | | | |

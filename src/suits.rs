//! Suit shapes defined as math (circles, triangles, stems) and rasterized to
//! hard-edged pixel masks at any size. Every suit symbol in the game (card
//! pips, UI icons, suits inside text) comes from here, so spades and clubs
//! keep distinct silhouettes even when small: clubs are three round lobes,
//! spades have a pointed tip.

use crate::euchre::Suit;

fn in_circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r
}

/// A flared stem from `top` down to `bottom` (= 1.0).
fn in_stem(x: f32, y: f32, top: f32, top_half_width: f32, bottom_half_width: f32) -> bool {
    if !(top..=0.98).contains(&y) {
        return false;
    }
    let t = (y - top) / (0.98 - top);
    let w = top_half_width + (bottom_half_width - top_half_width) * t * t;
    x.abs() <= w
}

/// Whether the point `(x, y)` (each in -1..1, y pointing down) is inside the suit shape.
pub fn contains(suit: Suit, x: f32, y: f32) -> bool {
    match suit {
        Suit::Diamonds => x.abs() / 0.74 + y.abs() <= 0.98,
        Suit::Hearts => {
            let lobes = in_circle(x, y, -0.45, -0.38, 0.49) || in_circle(x, y, 0.45, -0.38, 0.49);
            let point = (-0.4..=0.95).contains(&y) && x.abs() <= 0.92 * (0.95 - y) / 1.35;
            lobes || point
        }
        Suit::Spades => {
            let tip = (-0.98..=0.2).contains(&y) && x.abs() <= 0.9 * (y + 0.98) / 1.18;
            let lobes = in_circle(x, y, -0.43, 0.18, 0.45) || in_circle(x, y, 0.43, 0.18, 0.45);
            lobes || tip || in_stem(x, y, 0.35, 0.09, 0.42)
        }
        Suit::Clubs => {
            let r = 0.37;
            let lobes = in_circle(x, y, 0.0, -0.55, r)
                || in_circle(x, y, -0.55, 0.1, r)
                || in_circle(x, y, 0.55, 0.1, r)
                || in_circle(x, y, 0.0, 0.0, 0.22);
            lobes || in_stem(x, y, 0.2, 0.09, 0.42)
        }
    }
}

/// Hand-tuned masks for the small sizes, where math shapes turn to blobs.
fn hand_drawn(suit: Suit, size: u32) -> Option<&'static [&'static str]> {
    let rows: &[&str] = match (size, suit) {
        (11, Suit::Spades) => &[
            ".....#.....",
            "....###....",
            "...#####...",
            "..#######..",
            ".#########.",
            "###########",
            "###########",
            ".###.#.###.",
            ".....#.....",
            "....###....",
            "...#####...",
        ],
        (11, Suit::Clubs) => &[
            "....###....",
            "...#####...",
            "...#####...",
            "...#####...",
            ".##.###.##.",
            "###########",
            "###########",
            "###########",
            ".##..#..##.",
            ".....#.....",
            "...#####...",
        ],
        (11, Suit::Hearts) => &[
            "...........",
            ".###...###.",
            "#####.#####",
            "###########",
            "###########",
            "###########",
            ".#########.",
            "..#######..",
            "...#####...",
            "....###....",
            ".....#.....",
        ],
        (11, Suit::Diamonds) => &[
            ".....#.....",
            "....###....",
            "....###....",
            "...#####...",
            "..#######..",
            ".#########.",
            "..#######..",
            "...#####...",
            "....###....",
            "....###....",
            ".....#.....",
        ],
        (13, Suit::Spades) => &[
            "......#......",
            ".....###.....",
            "....#####....",
            "...#######...",
            "..#########..",
            ".###########.",
            "#############",
            "#############",
            "#############",
            ".####.#.####.",
            "......#......",
            ".....###.....",
            "...#######...",
        ],
        (13, Suit::Clubs) => &[
            ".....###.....",
            "....#####....",
            "...#######...",
            "...#######...",
            "....#####....",
            ".###.###.###.",
            "#############",
            "#############",
            "#############",
            ".###..#..###.",
            "......#......",
            ".....###.....",
            "...#######...",
        ],
        (13, Suit::Hearts) => &[
            ".............",
            ".####...####.",
            "######.######",
            "#############",
            "#############",
            "#############",
            ".###########.",
            "..#########..",
            "...#######...",
            "....#####....",
            ".....###.....",
            "......#......",
            ".............",
        ],
        (13, Suit::Diamonds) => &[
            "......#......",
            ".....###.....",
            "....#####....",
            "....#####....",
            "...#######...",
            "..#########..",
            ".###########.",
            "..#########..",
            "...#######...",
            "....#####....",
            "....#####....",
            ".....###.....",
            "......#......",
        ],
        _ => return None,
    };
    Some(rows)
}

/// Rasterizes the suit into a `size x size` mask (row-major, `true` = ink).
/// Sizes 11 and 13 are hand-drawn; larger ones are supersampled from the
/// math shapes and kept where at least half of a pixel is covered, which
/// gives clean, hard pixel-art edges.
pub fn mask(suit: Suit, size: u32) -> Vec<bool> {
    if let Some(rows) = hand_drawn(suit, size) {
        return rows.iter().flat_map(|r| r.bytes().map(|b| b == b'#')).collect();
    }
    const SS: u32 = 4;
    let n = size as f32;
    let mut out = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (px as f32 + (sx as f32 + 0.5) / SS as f32) / n * 2.0 - 1.0;
                    let fy = (py as f32 + (sy as f32 + 0.5) / SS as f32) / n * 2.0 - 1.0;
                    if contains(suit, fx, fy) {
                        hits += 1;
                    }
                }
            }
            out.push(hits * 2 >= SS * SS);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(suit: Suit, size: u32) -> String {
        let m = mask(suit, size);
        m.chunks(size as usize)
            .map(|row| row.iter().map(|&b| if b { '#' } else { '.' }).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn spades_and_clubs_differ_at_every_size() {
        for size in [9, 11, 13, 19, 27] {
            assert_ne!(mask(Suit::Spades, size), mask(Suit::Clubs, size), "size {size}");
        }
    }

    #[test]
    fn shapes_are_symmetric() {
        for (suit, size) in Suit::ALL.into_iter().flat_map(|s| [11, 13, 19, 27, 39].map(|n| (s, n))) {
            let m = mask(suit, size);
            assert_eq!(m.len(), (size * size) as usize);
            for row in m.chunks(size as usize) {
                let rev: Vec<bool> = row.iter().rev().copied().collect();
                assert_eq!(row, rev.as_slice(), "{suit:?} is lopsided:\n{}", render(suit, size));
            }
        }
    }

    /// `cargo test print_suits -- --ignored --nocapture` to eyeball the shapes.
    #[test]
    #[ignore]
    fn print_suits() {
        for size in [19, 27, 39] {
            for suit in Suit::ALL {
                println!("{suit:?} {size}\n{}\n", render(suit, size));
            }
        }
    }
}

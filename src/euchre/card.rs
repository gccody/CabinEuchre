//! Cards, suits and ranks, plus the trump-aware ordering that makes Euchre
//! Euchre: the jack of trump (right bower) is the highest card, and the jack of
//! the other suit of the same color (left bower) is second highest and counts
//! as a trump card, not as a card of its printed suit.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Suit {
    Clubs,
    Diamonds,
    Spades,
    Hearts,
}

impl Suit {
    /// Ordered so colors alternate, which also makes a nicely sorted hand.
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Spades, Suit::Hearts];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn is_red(self) -> bool {
        matches!(self, Suit::Diamonds | Suit::Hearts)
    }

    /// The other suit of the same color (where the left bower comes from).
    pub fn same_color(self) -> Suit {
        match self {
            Suit::Clubs => Suit::Spades,
            Suit::Spades => Suit::Clubs,
            Suit::Diamonds => Suit::Hearts,
            Suit::Hearts => Suit::Diamonds,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Suit::Clubs => "Clubs",
            Suit::Diamonds => "Diamonds",
            Suit::Spades => "Spades",
            Suit::Hearts => "Hearts",
        }
    }

    pub fn symbol(self) -> char {
        match self {
            Suit::Clubs => '♣',
            Suit::Diamonds => '♦',
            Suit::Spades => '♠',
            Suit::Hearts => '♥',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rank {
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    pub const ALL: [Rank; 6] = [Rank::Nine, Rank::Ten, Rank::Jack, Rank::Queen, Rank::King, Rank::Ace];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Rank::Nine => "9",
            Rank::Ten => "10",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
            Rank::Ace => "A",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

/// Number of cards in a Euchre deck.
pub const DECK_SIZE: usize = 24;

impl Card {
    pub const fn new(rank: Rank, suit: Suit) -> Self {
        Card { suit, rank }
    }

    /// Stable index in `0..24`, handy for lookup tables.
    pub fn id(self) -> usize {
        self.suit.index() * 6 + self.rank.index()
    }

    pub fn from_id(id: usize) -> Card {
        Card::new(Rank::ALL[id % 6], Suit::ALL[id / 6])
    }

    /// The full 24-card deck, in id order.
    pub fn deck() -> Vec<Card> {
        (0..DECK_SIZE).map(Card::from_id).collect()
    }

    pub fn is_right_bower(self, trump: Suit) -> bool {
        self.rank == Rank::Jack && self.suit == trump
    }

    pub fn is_left_bower(self, trump: Suit) -> bool {
        self.rank == Rank::Jack && self.suit == trump.same_color()
    }

    /// The suit this card belongs to once trump is known (the left bower is trump).
    pub fn suit_under(self, trump: Suit) -> Suit {
        if self.is_left_bower(trump) { trump } else { self.suit }
    }

    pub fn is_trump(self, trump: Suit) -> bool {
        self.suit_under(trump) == trump
    }

    /// Trick-taking strength when `led` (an effective suit) was led.
    /// `0` means the card cannot win the trick. Trump cards are `11..=17`
    /// (right bower 17, left bower 16); cards of the led suit are `1..=6`.
    pub fn power(self, trump: Suit, led: Suit) -> u8 {
        if self.is_right_bower(trump) {
            17
        } else if self.is_left_bower(trump) {
            16
        } else if self.suit == trump {
            10 + match self.rank {
                Rank::Nine => 1,
                Rank::Ten => 2,
                Rank::Queen => 3,
                Rank::King => 4,
                Rank::Ace => 5,
                Rank::Jack => unreachable!("jack of trump is the right bower"),
            }
        } else if self.suit == led {
            1 + self.rank.index() as u8
        } else {
            0
        }
    }

    /// Short label such as `"J♥"`.
    pub fn label(self) -> String {
        format!("{}{}", self.rank.label(), self.suit.symbol())
    }
}

impl fmt::Display for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(rank: Rank, suit: Suit) -> Card {
        Card::new(rank, suit)
    }

    #[test]
    fn ids_round_trip() {
        for id in 0..DECK_SIZE {
            assert_eq!(Card::from_id(id).id(), id);
        }
        assert_eq!(Card::deck().len(), 24);
    }

    #[test]
    fn bowers_are_the_top_trumps() {
        let trump = Suit::Hearts;
        let right = c(Rank::Jack, Suit::Hearts);
        let left = c(Rank::Jack, Suit::Diamonds);
        let ace = c(Rank::Ace, Suit::Hearts);
        assert!(right.is_right_bower(trump));
        assert!(left.is_left_bower(trump));
        assert!(right.power(trump, Suit::Hearts) > left.power(trump, Suit::Hearts));
        assert!(left.power(trump, Suit::Hearts) > ace.power(trump, Suit::Hearts));
        // Even when another suit was led.
        assert!(left.power(trump, Suit::Clubs) > c(Rank::Ace, Suit::Clubs).power(trump, Suit::Clubs));
    }

    #[test]
    fn left_bower_belongs_to_trump() {
        let left = c(Rank::Jack, Suit::Spades);
        assert_eq!(left.suit_under(Suit::Clubs), Suit::Clubs);
        assert!(left.is_trump(Suit::Clubs));
        // ...and is no longer a spade.
        assert_eq!(left.power(Suit::Clubs, Suit::Spades), 16);
        assert_ne!(left.suit_under(Suit::Clubs), Suit::Spades);
        // The other-color jack is just a jack.
        let red_jack = c(Rank::Jack, Suit::Hearts);
        assert!(!red_jack.is_trump(Suit::Clubs));
        assert_eq!(red_jack.power(Suit::Clubs, Suit::Hearts), 3);
    }

    #[test]
    fn off_suit_cards_cannot_win() {
        let trump = Suit::Spades;
        assert_eq!(c(Rank::Ace, Suit::Hearts).power(trump, Suit::Clubs), 0);
        assert_eq!(c(Rank::Nine, Suit::Spades).power(trump, Suit::Clubs), 11);
    }

    #[test]
    fn trump_order() {
        let trump = Suit::Diamonds;
        let order = [
            c(Rank::Nine, Suit::Diamonds),
            c(Rank::Ten, Suit::Diamonds),
            c(Rank::Queen, Suit::Diamonds),
            c(Rank::King, Suit::Diamonds),
            c(Rank::Ace, Suit::Diamonds),
            c(Rank::Jack, Suit::Hearts),
            c(Rank::Jack, Suit::Diamonds),
        ];
        for w in order.windows(2) {
            assert!(w[0].power(trump, trump) < w[1].power(trump, trump), "{} < {}", w[0], w[1]);
        }
    }
}

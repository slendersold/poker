use rand::seq::SliceRandom;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Card {
    pub rank: u8,
    pub suit: Suit,
}

impl Card {
    pub fn rank_text(self) -> &'static str {
        match self.rank {
            14 => "A",
            13 => "K",
            12 => "Q",
            11 => "J",
            10 => "10",
            9 => "9",
            8 => "8",
            7 => "7",
            6 => "6",
            5 => "5",
            4 => "4",
            3 => "3",
            _ => "2",
        }
    }
    pub fn suit_text(self) -> &'static str {
        match self.suit {
            Suit::Clubs => "♣",
            Suit::Diamonds => "♦",
            Suit::Hearts => "♥",
            Suit::Spades => "♠",
        }
    }
    pub fn is_red(self) -> bool {
        matches!(self.suit, Suit::Diamonds | Suit::Hearts)
    }
}

pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn shuffled() -> Self {
        let mut cards = Vec::with_capacity(52);
        for suit in [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades] {
            for rank in 2..=14 {
                cards.push(Card { rank, suit });
            }
        }
        cards.shuffle(&mut rand::rng());
        Self { cards }
    }
    pub fn draw(&mut self) -> Card {
        self.cards.pop().expect("deck is not empty")
    }
}

/// Comparable score: category in the high byte, then five rank kickers.
pub fn evaluate(cards: &[Card]) -> u64 {
    best_five(cards).0
}

/// Returns the score and the exact five cards that form the strongest hand.
pub fn best_five(cards: &[Card]) -> (u64, Vec<Card>) {
    assert!(cards.len() >= 5);
    let mut best = 0;
    let mut winning = Vec::with_capacity(5);
    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        let five = [cards[a], cards[b], cards[c], cards[d], cards[e]];
                        let score = evaluate_five(five);
                        if score > best {
                            best = score;
                            winning = five.to_vec();
                        }
                    }
                }
            }
        }
    }
    (best, winning)
}

fn packed(category: u8, ranks: &[u8]) -> u64 {
    let mut score = (category as u64) << 24;
    for (index, &rank) in ranks.iter().take(5).enumerate() {
        score |= (rank as u64) << (20 - index * 4);
    }
    score
}

fn evaluate_five(cards: [Card; 5]) -> u64 {
    let mut counts = [0u8; 15];
    for c in cards {
        counts[c.rank as usize] += 1;
    }
    let flush = cards.iter().all(|c| c.suit == cards[0].suit);
    let mut unique: Vec<u8> = (2..=14)
        .filter(|&r| counts[r] > 0)
        .map(|r| r as u8)
        .collect();
    if unique.contains(&14) {
        unique.insert(0, 1);
    }
    let straight_high = unique
        .windows(5)
        .filter(|w| w.windows(2).all(|x| x[1] == x[0] + 1))
        .map(|w| w[4])
        .max();
    if let (true, Some(high)) = (flush, straight_high) {
        return packed(8, &[high]);
    }
    let mut groups: Vec<(u8, u8)> = (2..=14)
        .filter(|&r| counts[r] > 0)
        .map(|r| (counts[r], r as u8))
        .collect();
    groups.sort_unstable_by(|a, b| b.cmp(a));
    if groups[0].0 == 4 {
        return packed(7, &[groups[0].1, groups[1].1]);
    }
    if groups[0].0 == 3 && groups[1].0 == 2 {
        return packed(6, &[groups[0].1, groups[1].1]);
    }
    let mut desc: Vec<u8> = (2..=14)
        .rev()
        .filter(|&r| counts[r] > 0)
        .map(|r| r as u8)
        .collect();
    if flush {
        return packed(5, &desc);
    }
    if let Some(high) = straight_high {
        return packed(4, &[high]);
    }
    if groups[0].0 == 3 {
        let mut r = vec![groups[0].1];
        r.extend(groups.iter().filter(|g| g.0 == 1).map(|g| g.1));
        return packed(3, &r);
    }
    let pairs: Vec<u8> = groups.iter().filter(|g| g.0 == 2).map(|g| g.1).collect();
    if pairs.len() >= 2 {
        let kicker = groups.iter().find(|g| g.0 == 1).unwrap().1;
        return packed(2, &[pairs[0], pairs[1], kicker]);
    }
    if pairs.len() == 1 {
        let mut r = vec![pairs[0]];
        r.extend(groups.iter().filter(|g| g.0 == 1).map(|g| g.1));
        return packed(1, &r);
    }
    desc.truncate(5);
    packed(0, &desc)
}

pub fn hand_name(score: u64) -> &'static str {
    match (score >> 24) as u8 {
        8 => "стрит-флеш",
        7 => "каре",
        6 => "фулл-хаус",
        5 => "флеш",
        4 => "стрит",
        3 => "сет",
        2 => "две пары",
        1 => "пара",
        _ => "старшая карта",
    }
}

/// Returns cards that only break ties inside the same made-hand category.
pub fn kicker_cards(five: &[Card], score: u64) -> Vec<Card> {
    let category = (score >> 24) as u8;
    let mut counts = [0u8; 15];
    for card in five {
        counts[card.rank as usize] += 1;
    }
    match category {
        // The top card names a high-card hand; the other four break ties.
        0 => {
            let highest = five.iter().map(|card| card.rank).max().unwrap_or(0);
            five.iter()
                .copied()
                .filter(|card| card.rank != highest)
                .collect()
        }
        // With a pair or trips, every unpaired card is a kicker.
        1 | 3 => five
            .iter()
            .copied()
            .filter(|card| counts[card.rank as usize] == 1)
            .collect(),
        // Two pair and quads have one ungrouped kicker.
        2 | 7 => five
            .iter()
            .copied()
            .filter(|card| counts[card.rank as usize] == 1)
            .collect(),
        // Straight, flush, full house, and straight flush use all five cards.
        _ => vec![],
    }
}

fn score_ranks(score: u64) -> Vec<u8> {
    (0..5)
        .map(|index| ((score >> (20 - index * 4)) & 0xF) as u8)
        .filter(|&rank| rank > 0)
        .collect()
}

fn rank_label(rank: u8) -> &'static str {
    match rank {
        14 => "A",
        13 => "K",
        12 => "Q",
        11 => "J",
        10 => "10",
        9 => "9",
        8 => "8",
        7 => "7",
        6 => "6",
        5 => "5",
        4 => "4",
        3 => "3",
        _ => "2",
    }
}

fn joined_ranks(ranks: &[u8]) -> String {
    ranks
        .iter()
        .map(|&rank| rank_label(rank))
        .collect::<Vec<_>>()
        .join("–")
}

pub fn describe_hand(score: u64) -> String {
    let ranks = score_ranks(score);
    match (score >> 24) as u8 {
        8 => format!("стрит-флеш до {}", rank_label(ranks[0])),
        7 => format!(
            "каре {}, кикер {}",
            rank_label(ranks[0]),
            rank_label(ranks[1])
        ),
        6 => format!(
            "фулл-хаус: {} поверх {}",
            rank_label(ranks[0]),
            rank_label(ranks[1])
        ),
        5 => format!("флеш {}", joined_ranks(&ranks)),
        4 => format!("стрит до {}", rank_label(ranks[0])),
        3 => format!(
            "сет {}, кикеры {}",
            rank_label(ranks[0]),
            joined_ranks(&ranks[1..])
        ),
        2 => format!(
            "две пары {} и {}, кикер {}",
            rank_label(ranks[0]),
            rank_label(ranks[1]),
            rank_label(ranks[2])
        ),
        1 => format!(
            "пара {}, кикеры {}",
            rank_label(ranks[0]),
            joined_ranks(&ranks[1..])
        ),
        _ => format!(
            "старшая {}, далее {}",
            rank_label(ranks[0]),
            joined_ranks(&ranks[1..])
        ),
    }
}

pub fn explain_loss(winner: u64, loser: u64) -> String {
    let winner_category = (winner >> 24) as u8;
    let loser_category = (loser >> 24) as u8;
    if winner_category != loser_category {
        return format!("{} сильнее, чем {}.", hand_name(winner), hand_name(loser));
    }
    let winner_ranks = score_ranks(winner);
    let loser_ranks = score_ranks(loser);
    let difference = winner_ranks
        .iter()
        .zip(&loser_ranks)
        .position(|(winner_rank, loser_rank)| winner_rank != loser_rank);
    let Some(index) = difference else {
        return "Пять лучших карт равны — банк должен быть разделён.".into();
    };
    let criterion = match winner_category {
        8 | 4 => "старшая карта стрита",
        7 => {
            if index == 0 {
                "ранг каре"
            } else {
                "кикер"
            }
        }
        6 => {
            if index == 0 {
                "ранг тройки"
            } else {
                "ранг пары"
            }
        }
        5 | 0 => {
            if index == 0 {
                "старшая карта"
            } else {
                "следующая карта"
            }
        }
        3 => {
            if index == 0 {
                "ранг сета"
            } else {
                "кикер"
            }
        }
        2 => match index {
            0 => "старшая пара",
            1 => "младшая пара",
            _ => "кикер",
        },
        1 => {
            if index == 0 {
                "ранг пары"
            } else {
                "кикер"
            }
        }
        _ => "решающая карта",
    };
    format!(
        "Решил {criterion}: {} против {}.",
        rank_label(winner_ranks[index]),
        rank_label(loser_ranks[index])
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(rank: u8, suit: Suit) -> Card {
        Card { rank, suit }
    }
    #[test]
    fn categories_are_ordered() {
        let sf = [
            c(10, Suit::Hearts),
            c(11, Suit::Hearts),
            c(12, Suit::Hearts),
            c(13, Suit::Hearts),
            c(14, Suit::Hearts),
        ];
        let quads = [
            c(9, Suit::Hearts),
            c(9, Suit::Clubs),
            c(9, Suit::Spades),
            c(9, Suit::Diamonds),
            c(2, Suit::Hearts),
        ];
        assert!(evaluate(&sf) > evaluate(&quads));
    }
    #[test]
    fn wheel_straight_works() {
        let wheel = [
            c(14, Suit::Hearts),
            c(2, Suit::Clubs),
            c(3, Suit::Spades),
            c(4, Suit::Diamonds),
            c(5, Suit::Hearts),
        ];
        assert_eq!((evaluate(&wheel) >> 24) as u8, 4);
    }

    #[test]
    fn best_five_returns_only_winning_cards() {
        let cards = [
            c(10, Suit::Hearts),
            c(11, Suit::Hearts),
            c(12, Suit::Hearts),
            c(13, Suit::Hearts),
            c(14, Suit::Hearts),
            c(14, Suit::Clubs),
            c(2, Suit::Spades),
        ];
        let (score, winning) = best_five(&cards);
        assert_eq!((score >> 24) as u8, 8);
        assert_eq!(winning.len(), 5);
        assert!(winning.iter().all(|card| card.suit == Suit::Hearts));
    }

    #[test]
    fn pair_has_three_kickers() {
        let five = [
            c(12, Suit::Spades),
            c(12, Suit::Clubs),
            c(13, Suit::Clubs),
            c(11, Suit::Spades),
            c(10, Suit::Clubs),
        ];
        let score = evaluate(&five);
        let kickers = kicker_cards(&five, score);
        assert_eq!(kickers.len(), 3);
        assert!(kickers.iter().all(|card| [10, 11, 13].contains(&card.rank)));
    }

    #[test]
    fn same_pair_is_explained_by_the_deciding_kicker() {
        let winner = evaluate(&[
            c(12, Suit::Spades),
            c(12, Suit::Clubs),
            c(13, Suit::Hearts),
            c(8, Suit::Diamonds),
            c(7, Suit::Clubs),
        ]);
        let loser = evaluate(&[
            c(12, Suit::Hearts),
            c(12, Suit::Diamonds),
            c(10, Suit::Spades),
            c(8, Suit::Clubs),
            c(7, Suit::Hearts),
        ]);
        let explanation = explain_loss(winner, loser);
        assert!(explanation.contains("кикер"));
        assert!(explanation.contains('K'));
        assert!(explanation.contains("10"));
    }
}

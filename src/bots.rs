use crate::cards::evaluate;
use crate::game::{Action, Game, Street};
use rand::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BotStyle {
    Careful,
    Aggressive,
    Tricky,
    Analytical,
}

impl BotStyle {
    pub const ALL: [Self; 4] = [
        Self::Careful,
        Self::Aggressive,
        Self::Tricky,
        Self::Analytical,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Careful => "осторожная",
            Self::Aggressive => "агрессор",
            Self::Tricky => "непредсказуемый",
            Self::Analytical => "аналитик",
        }
    }
}

pub fn decide(game: &Game, i: usize) -> Action {
    let p = &game.players[i];
    let call = game.to_call(i);
    let mut rng = rand::rng();
    let style = p.bot.unwrap();
    let strength = if game.street == Street::Preflop {
        preflop(&p.hole)
    } else {
        let mut c = game.community.clone();
        c.extend(&p.hole);
        ((evaluate(&c) >> 24) as f32 / 8.0).max(preflop(&p.hole) * 0.45)
    };
    let noise: f32 = rng.random_range(-0.12..0.12);
    let (fold_bar, raise_bar, bluff) = match style {
        BotStyle::Careful => (0.34, 0.70, 0.02),
        BotStyle::Aggressive => (0.18, 0.48, 0.18),
        BotStyle::Tricky => (0.25, 0.62, 0.28),
        BotStyle::Analytical => (0.28, 0.58, 0.07),
    };
    let pressure = call as f32 / (p.stack + call).max(1) as f32;
    if call > 0 && strength + noise < fold_bar + pressure * 0.55 {
        return Action::Fold;
    }
    if strength + noise > raise_bar || rng.random_bool(bluff) {
        let factor = match style {
            BotStyle::Aggressive => rng.random_range(3..=6),
            BotStyle::Tricky => rng.random_range(2..=5),
            _ => rng.random_range(2..=4),
        };
        return Action::Raise(game.current_bet + game.min_raise.max(game.big_blind) * factor / 2);
    }
    Action::CheckCall
}

fn preflop(h: &[crate::cards::Card]) -> f32 {
    if h.len() != 2 {
        return 0.0;
    }
    let hi = h[0].rank.max(h[1].rank) as f32;
    let lo = h[0].rank.min(h[1].rank) as f32;
    let pair = h[0].rank == h[1].rank;
    let suited = h[0].suit == h[1].suit;
    let gap = (hi - lo).abs();
    let mut s = (hi - 2.0) / 12.0 * 0.55 + (lo - 2.0) / 12.0 * 0.18;
    if pair {
        s += 0.28 + hi / 100.0;
    }
    if suited {
        s += 0.07;
    }
    if gap <= 2.0 {
        s += 0.06;
    }
    s.min(1.0)
}

use crate::bots::BotStyle;
use crate::cards::{
    Card, Deck, best_five, describe_hand, evaluate, explain_loss, hand_name, kicker_cards,
};

pub const SMALL_BLIND: u32 = 10;
pub const BIG_BLIND: u32 = 20;

#[derive(Clone, Copy)]
pub struct GameConfig {
    pub starting_stack: u32,
    pub small_blind: u32,
    pub tournament_mode: bool,
    pub hands_per_level: u32,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            starting_stack: 1500,
            small_blind: SMALL_BLIND,
            tournament_mode: false,
            hands_per_level: 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
    Showdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heads_up_dealer_posts_small_blind_and_acts_first_preflop() {
        let game = Game::with_bots(&[BotStyle::Careful], GameConfig::default());
        assert_eq!(game.players.len(), 2);
        assert_eq!(game.players[game.dealer].street_bet, SMALL_BLIND);
        assert_eq!(game.current, game.dealer);
        assert_eq!(game.players[(game.dealer + 1) % 2].street_bet, BIG_BLIND);
    }

    #[test]
    fn human_with_zero_stack_ends_the_tournament() {
        let mut game = Game::with_bots(
            &[BotStyle::Careful, BotStyle::Aggressive],
            GameConfig::default(),
        );
        game.players[0].stack = 0;
        assert!(game.update_tournament_state());
        assert!(game.game_over);
        assert!(!game.human_won);
        assert!(game.players[0].eliminated);
    }

    #[test]
    fn busted_bot_is_skipped_in_the_next_hand() {
        let mut game = Game::with_bots(
            &[BotStyle::Careful, BotStyle::Aggressive],
            GameConfig::default(),
        );
        game.players[1].stack = 0;
        assert!(!game.update_tournament_state());
        game.start_hand();
        assert!(game.players[1].eliminated);
        assert!(game.players[1].hole.is_empty());
        assert!(!game.game_over);
    }

    #[test]
    fn uncontested_pot_records_the_winners_payout() {
        let mut game = Game::with_bots(
            &[BotStyle::Careful, BotStyle::Aggressive],
            GameConfig::default(),
        );
        game.pot = 120;
        for player in game.players.iter_mut().skip(1) {
            player.folded = true;
        }
        let before = game.players[0].stack;
        game.advance();
        assert_eq!(game.last_payouts[0], 120);
        assert_eq!(game.players[0].stack, before + 120);
        assert_eq!(game.pot, 0);
    }

    #[test]
    fn tournament_blinds_double_after_configured_number_of_hands() {
        let config = GameConfig {
            tournament_mode: true,
            hands_per_level: 2,
            ..GameConfig::default()
        };
        let mut game = Game::with_bots(&[BotStyle::Careful, BotStyle::Aggressive], config);
        assert_eq!(game.hand_number, 1);
        assert_eq!((game.small_blind, game.big_blind), (10, 20));
        game.start_hand();
        assert_eq!(game.hand_number, 2);
        assert_eq!((game.small_blind, game.big_blind), (10, 20));
        game.start_hand();
        assert_eq!(game.hand_number, 3);
        assert_eq!((game.small_blind, game.big_blind), (20, 40));
    }

    #[test]
    fn raise_bounds_include_full_minimum_and_short_all_in() {
        let mut game = Game::with_bots(&[BotStyle::Careful], GameConfig::default());
        let player = game.current;
        game.current_bet = 100;
        game.min_raise = 40;
        game.players[player].street_bet = 20;
        game.players[player].stack = 500;
        assert_eq!(game.raise_bounds(player), Some((140, 520)));
        game.players[player].stack = 100;
        assert_eq!(game.raise_bounds(player), Some((120, 120)));
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Fold,
    CheckCall,
    Raise(u32),
}

#[derive(Clone)]
pub struct Player {
    pub name: String,
    pub stack: u32,
    pub hole: Vec<Card>,
    pub folded: bool,
    pub all_in: bool,
    pub street_bet: u32,
    pub contributed: u32,
    pub acted: bool,
    pub eliminated: bool,
    pub bot: Option<BotStyle>,
}

pub struct Game {
    pub players: Vec<Player>,
    pub community: Vec<Card>,
    pub pot: u32,
    pub dealer: usize,
    pub current: usize,
    pub current_bet: u32,
    pub min_raise: u32,
    pub street: Street,
    pub message: String,
    pub reveal_all: bool,
    pub highlighted_cards: Vec<Card>,
    pub kicker_cards: Vec<Card>,
    pub last_payouts: Vec<u32>,
    pub game_over: bool,
    pub human_won: bool,
    pub hand_analysis: Vec<String>,
    pub small_blind: u32,
    pub big_blind: u32,
    pub hand_number: u32,
    pub blind_level: u32,
    config: GameConfig,
    deck: Deck,
}

impl Game {
    pub fn with_bots(styles: &[BotStyle], config: GameConfig) -> Self {
        assert!((1..=4).contains(&styles.len()));
        let names = ["Марина", "Борис", "Лис", "Профессор"];
        let mut seats = vec![("Вы", None)];
        seats.extend(
            styles
                .iter()
                .enumerate()
                .map(|(index, &style)| (names[index], Some(style))),
        );
        let players: Vec<Player> = seats
            .into_iter()
            .map(|(name, bot)| Player {
                name: name.into(),
                stack: config.starting_stack,
                hole: vec![],
                folded: false,
                all_in: false,
                street_bet: 0,
                contributed: 0,
                acted: false,
                eliminated: false,
                bot,
            })
            .collect();
        let player_count = players.len();
        let mut game = Self {
            players,
            community: vec![],
            pot: 0,
            dealer: 0,
            current: 0,
            current_bet: 0,
            min_raise: BIG_BLIND,
            street: Street::Showdown,
            message: String::new(),
            reveal_all: false,
            highlighted_cards: vec![],
            kicker_cards: vec![],
            last_payouts: vec![0; player_count],
            game_over: false,
            human_won: false,
            hand_analysis: vec![],
            small_blind: config.small_blind,
            big_blind: config.small_blind.saturating_mul(2),
            hand_number: 0,
            blind_level: 1,
            config,
            deck: Deck::shuffled(),
        };
        game.start_hand();
        game
    }

    pub fn start_hand(&mut self) {
        for p in &mut self.players {
            p.eliminated |= p.stack == 0;
            p.hole.clear();
            p.folded = p.eliminated;
            p.all_in = false;
            p.street_bet = 0;
            p.contributed = 0;
            p.acted = p.eliminated;
        }
        self.last_payouts.fill(0);
        if self.update_tournament_state() {
            return;
        }
        self.hand_number += 1;
        if self.config.tournament_mode
            && self.hand_number > 1
            && (self.hand_number - 1).is_multiple_of(self.config.hands_per_level.max(1))
        {
            self.small_blind = self.small_blind.saturating_mul(2);
            self.big_blind = self.big_blind.saturating_mul(2);
            self.blind_level = self.blind_level.saturating_add(1);
        }
        self.dealer = self.next_active(self.dealer);
        self.community.clear();
        self.pot = 0;
        self.current_bet = 0;
        self.min_raise = self.big_blind;
        self.street = Street::Preflop;
        self.reveal_all = false;
        self.highlighted_cards.clear();
        self.kicker_cards.clear();
        self.hand_analysis.clear();
        self.deck = Deck::shuffled();
        for _ in 0..2 {
            let mut i = self.next_active(self.dealer);
            for _ in 0..self.active_count() {
                self.players[i].hole.push(self.deck.draw());
                i = self.next_active(i);
            }
        }
        let heads_up = self.active_count() == 2;
        let sb = if heads_up {
            self.dealer
        } else {
            self.next_active(self.dealer)
        };
        let bb = self.next_active(sb);
        self.commit(sb, self.small_blind);
        self.commit(bb, self.big_blind);
        self.current_bet = self.big_blind;
        self.min_raise = self.big_blind;
        self.current = if heads_up {
            self.dealer
        } else {
            self.next_active(bb)
        };
        self.message = format!(
            "Раздача {} · уровень {} · блайнды {}/{}",
            self.hand_number, self.blind_level, self.small_blind, self.big_blind
        );
    }

    fn active_count(&self) -> usize {
        self.players.iter().filter(|p| !p.eliminated).count()
    }

    fn next_active(&self, from: usize) -> usize {
        for offset in 1..=self.players.len() {
            let index = (from + offset) % self.players.len();
            if !self.players[index].eliminated {
                return index;
            }
        }
        from
    }

    /// Marks busted stacks and reports whether the tournament has ended.
    fn update_tournament_state(&mut self) -> bool {
        for player in &mut self.players {
            if player.stack == 0 {
                player.eliminated = true;
                player.folded = true;
            }
        }
        let remaining = self.active_count();
        self.game_over = self.players[0].eliminated || remaining <= 1;
        self.human_won = self.game_over && !self.players[0].eliminated;
        if self.game_over {
            self.street = Street::Showdown;
            self.message = if self.human_won {
                "Вы выиграли турнир! Все соперники выбыли.".into()
            } else {
                "Фишки закончились — вы выбыли из игры.".into()
            };
        }
        self.game_over
    }

    fn commit(&mut self, i: usize, amount: u32) {
        let paid = amount.min(self.players[i].stack);
        self.players[i].stack -= paid;
        self.players[i].street_bet += paid;
        self.players[i].contributed += paid;
        self.pot += paid;
        if self.players[i].stack == 0 {
            self.players[i].all_in = true;
        }
    }

    pub fn to_call(&self, i: usize) -> u32 {
        self.current_bet.saturating_sub(self.players[i].street_bet)
    }

    /// Inclusive legal raise-to range. A short stack gets one all-in point
    /// below the normal minimum; otherwise the lower bound is a full raise.
    pub fn raise_bounds(&self, i: usize) -> Option<(u32, u32)> {
        let all_in_total = self.players[i].street_bet + self.players[i].stack;
        if all_in_total <= self.current_bet {
            return None;
        }
        let full_minimum = self.current_bet.saturating_add(self.min_raise);
        Some((full_minimum.min(all_in_total), all_in_total))
    }
    pub fn can_act(&self, i: usize) -> bool {
        !self.players[i].eliminated
            && !self.players[i].folded
            && !self.players[i].all_in
            && self.street != Street::Showdown
    }
    pub fn is_human_turn(&self) -> bool {
        self.current == 0 && self.can_act(0)
    }

    pub fn act(&mut self, action: Action) {
        if !self.can_act(self.current) {
            return;
        }
        self.last_payouts.fill(0);
        let i = self.current;
        match action {
            Action::Fold => {
                self.players[i].folded = true;
                self.players[i].acted = true;
                self.message = format!("{} сбрасывает", self.players[i].name);
            }
            Action::CheckCall => {
                let call = self.to_call(i);
                self.commit(i, call);
                self.players[i].acted = true;
                self.message = if call == 0 {
                    format!("{} — чек", self.players[i].name)
                } else {
                    format!(
                        "{} уравнивает {}",
                        self.players[i].name,
                        call.min(call + self.players[i].stack)
                    )
                };
            }
            Action::Raise(requested) => {
                let max_total = self.players[i].street_bet + self.players[i].stack;
                let target = requested
                    .max(self.current_bet + self.min_raise)
                    .min(max_total);
                if target <= self.current_bet {
                    self.commit(i, self.to_call(i));
                    self.players[i].acted = true;
                } else {
                    let raise_size = target - self.current_bet;
                    let add = target - self.players[i].street_bet;
                    self.commit(i, add);
                    self.current_bet = target;
                    self.min_raise = raise_size;
                    for (j, p) in self.players.iter_mut().enumerate() {
                        if j != i && !p.folded && !p.all_in {
                            p.acted = false;
                        }
                    }
                    self.players[i].acted = true;
                    self.message = format!("{} повышает до {}", self.players[i].name, target);
                }
            }
        }
        self.advance();
    }

    fn advance(&mut self) {
        let live: Vec<usize> = self
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.folded)
            .map(|(i, _)| i)
            .collect();
        if live.len() == 1 {
            let w = live[0];
            let prize = self.pot;
            self.players[w].stack += prize;
            self.last_payouts[w] += prize;
            self.message = format!("{} забирает банк {}", self.players[w].name, prize);
            self.hand_analysis.clear();
            if w == 0 {
                self.hand_analysis.push(format!(
                    "Вы выиграли {} без вскрытия: все соперники сбросили карты.",
                    prize
                ));
                self.hand_analysis.push(
                    "Второе место по силе определить нельзя — карты соперников не вскрывались."
                        .into(),
                );
            } else {
                self.hand_analysis.push(format!(
                    "{} выиграл банк {} без вскрытия.",
                    self.players[w].name, prize
                ));
                self.hand_analysis.push(
                    "Вы проиграли после паса: сброшенная рука больше не участвует в сравнении."
                        .into(),
                );
                let mut your_cards = self.community.clone();
                your_cards.extend(&self.players[0].hole);
                if your_cards.len() >= 5 {
                    self.hand_analysis.push(format!(
                        "На открытых к этому моменту картах у вас могла получиться: {}.",
                        describe_hand(evaluate(&your_cards))
                    ));
                }
            }
            self.pot = 0;
            self.street = Street::Showdown;
            self.update_tournament_state();
            return;
        }
        let round_done = self
            .players
            .iter()
            .filter(|p| !p.folded && !p.all_in)
            .all(|p| p.acted && p.street_bet == self.current_bet);
        if round_done {
            self.next_street();
            return;
        }
        for n in 1..=self.players.len() {
            let j = (self.current + n) % self.players.len();
            if self.can_act(j)
                && (!self.players[j].acted || self.players[j].street_bet != self.current_bet)
            {
                self.current = j;
                return;
            }
        }
        self.next_street();
    }

    fn next_street(&mut self) {
        for p in &mut self.players {
            p.street_bet = 0;
            p.acted = p.folded || p.all_in;
        }
        self.current_bet = 0;
        self.min_raise = self.big_blind;
        match self.street {
            Street::Preflop => {
                self.deck.draw();
                for _ in 0..3 {
                    self.community.push(self.deck.draw());
                }
                self.street = Street::Flop;
                self.message = "Флоп".into();
            }
            Street::Flop => {
                self.deck.draw();
                self.community.push(self.deck.draw());
                self.street = Street::Turn;
                self.message = "Тёрн".into();
            }
            Street::Turn => {
                self.deck.draw();
                self.community.push(self.deck.draw());
                self.street = Street::River;
                self.message = "Ривер".into();
            }
            Street::River => {
                self.showdown();
                return;
            }
            Street::Showdown => return,
        }
        let actionable = self
            .players
            .iter()
            .filter(|p| !p.folded && !p.all_in)
            .count();
        if actionable <= 1 {
            self.next_street();
            return;
        }
        for n in 1..=self.players.len() {
            let j = (self.dealer + n) % self.players.len();
            if self.can_act(j) {
                self.current = j;
                break;
            }
        }
    }

    fn showdown(&mut self) {
        self.street = Street::Showdown;
        self.reveal_all = true;
        let raw_scores: Vec<u64> = self
            .players
            .iter()
            .map(|p| {
                let mut cards = self.community.clone();
                cards.extend(&p.hole);
                evaluate(&cards)
            })
            .collect();
        let scores: Vec<u64> = raw_scores
            .iter()
            .enumerate()
            .map(|(index, &score)| if self.players[index].folded { 0 } else { score })
            .collect();
        let best = *scores.iter().max().unwrap();
        let headline: Vec<String> = scores
            .iter()
            .enumerate()
            .filter(|(i, score)| !self.players[*i].folded && **score == best)
            .map(|(i, _)| self.players[i].name.clone())
            .collect();
        self.highlighted_cards.clear();
        self.kicker_cards.clear();
        for (i, score) in scores.iter().enumerate() {
            if !self.players[i].folded && *score == best {
                let mut cards = self.community.clone();
                cards.extend(&self.players[i].hole);
                let (hand_score, five) = best_five(&cards);
                for card in kicker_cards(&five, hand_score) {
                    if !self.kicker_cards.contains(&card) {
                        self.kicker_cards.push(card);
                    }
                }
                for card in five {
                    if !self.highlighted_cards.contains(&card) {
                        self.highlighted_cards.push(card);
                    }
                }
            }
        }
        self.hand_analysis.clear();
        let best_players: Vec<usize> = scores
            .iter()
            .enumerate()
            .filter(|(index, score)| !self.players[*index].folded && **score == best)
            .map(|(index, _)| index)
            .collect();
        if best_players.contains(&0) {
            self.hand_analysis.push(if best_players.len() == 1 {
                format!(
                    "Вы победили. Ваша лучшая комбинация: {}.",
                    describe_hand(best)
                )
            } else {
                format!(
                    "Вы разделили первое место. Ваша лучшая комбинация: {}.",
                    describe_hand(best)
                )
            });
            let second = scores
                .iter()
                .enumerate()
                .filter(|(index, score)| {
                    !self.players[*index].folded && **score < best && **score > 0
                })
                .max_by_key(|(_, score)| **score)
                .map(|(index, &score)| (index, score));
            if let Some((index, score)) = second {
                self.hand_analysis.push(format!(
                    "Второе место — {}: {}.",
                    self.players[index].name,
                    describe_hand(score)
                ));
                self.hand_analysis.push(format!(
                    "Почему {} проиграл: {}",
                    self.players[index].name,
                    explain_loss(best, score)
                ));
            } else {
                self.hand_analysis.push(
                    "Отдельного второго места нет: остальные участники разделили первое место или сбросили карты."
                        .into(),
                );
            }
        } else {
            let winner = best_players[0];
            self.hand_analysis.push(format!(
                "Победил {}: {}.",
                self.players[winner].name,
                describe_hand(best)
            ));
            self.hand_analysis.push(format!(
                "Ваша лучшая комбинация: {}.",
                describe_hand(raw_scores[0])
            ));
            if self.players[0].folded {
                self.hand_analysis.push(
                    "Вы проиграли из-за паса: после сброса даже потенциально сильная рука не участвует в сравнении."
                        .into(),
                );
            } else {
                self.hand_analysis.push(format!(
                    "Почему вы проиграли: {}",
                    explain_loss(best, raw_scores[0])
                ));
            }
        }
        let total_pot = self.pot;

        // Each contribution level forms a side pot. Folded players fund it,
        // but cannot win it; this handles several different all-in stacks.
        let mut levels: Vec<u32> = self
            .players
            .iter()
            .map(|p| p.contributed)
            .filter(|&value| value > 0)
            .collect();
        levels.sort_unstable();
        levels.dedup();
        let mut previous = 0;
        for level in levels {
            let participants: Vec<usize> = self
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| p.contributed >= level)
                .map(|(i, _)| i)
                .collect();
            let amount = (level - previous) * participants.len() as u32;
            let eligible: Vec<usize> = participants
                .into_iter()
                .filter(|&i| !self.players[i].folded)
                .collect();
            if !eligible.is_empty() {
                let winning_score = eligible.iter().map(|&i| scores[i]).max().unwrap();
                let winners: Vec<usize> = eligible
                    .into_iter()
                    .filter(|&i| scores[i] == winning_score)
                    .collect();
                let share = amount / winners.len() as u32;
                let remainder = amount % winners.len() as u32;
                for (n, &i) in winners.iter().enumerate() {
                    let payout = share + u32::from(n == 0) * remainder;
                    self.players[i].stack += payout;
                    self.last_payouts[i] += payout;
                }
            }
            previous = level;
        }
        self.message = format!(
            "{} выигрывает шоудаун {} — {}",
            headline.join(" и "),
            total_pot,
            hand_name(best)
        );
        self.pot = 0;
        self.update_tournament_state();
    }
}

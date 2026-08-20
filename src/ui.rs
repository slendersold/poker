use crate::bots::{self, BotStyle};
use crate::cards::Card;
use crate::game::{Action, Game, GameConfig, Street};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use rand::seq::SliceRandom;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct TableSettings {
    hide_personalities: bool,
    randomize_bots: bool,
    bot_count: usize,
    selected: [BotStyle; 4],
    starting_stack: u32,
    small_blind: u32,
    tournament_mode: bool,
    hands_per_level: u32,
}

impl Default for TableSettings {
    fn default() -> Self {
        Self {
            hide_personalities: true,
            randomize_bots: true,
            bot_count: 4,
            selected: BotStyle::ALL,
            starting_stack: 1500,
            small_blind: 10,
            tournament_mode: false,
            hands_per_level: 5,
        }
    }
}

#[derive(Clone, Copy)]
enum ChipMotionKind {
    Bet,
    Gather,
    Award,
}

struct ChipMotion {
    player: usize,
    amount: u32,
    started: Instant,
    duration: Duration,
    kind: ChipMotionKind,
}

struct CommunityReveal {
    index: usize,
    started: Instant,
    duration: Duration,
}

pub struct PokerApp {
    game: Game,
    raise_to: u32,
    raise_cap: u32,
    raise_window_open: bool,
    raise_all_in_limit: bool,
    bot_due: Instant,
    settings: TableSettings,
    settings_open: bool,
    chip_motions: Vec<ChipMotion>,
    community_reveals: Vec<CommunityReveal>,
    analysis_open: bool,
}

impl PokerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        cc.egui_ctx.set_style(style);
        let settings = TableSettings::default();
        let styles = Self::resolved_styles(&settings);
        let game = Game::with_bots(&styles, Self::game_config(&settings));
        let raise_to = game.big_blind * 2;
        Self {
            game,
            raise_to,
            raise_cap: 1000,
            raise_window_open: false,
            raise_all_in_limit: false,
            bot_due: Instant::now() + Duration::from_millis(650),
            settings,
            settings_open: false,
            chip_motions: vec![],
            community_reveals: vec![],
            analysis_open: false,
        }
    }

    fn resolved_styles(settings: &TableSettings) -> Vec<BotStyle> {
        let mut styles = settings.selected.to_vec();
        if settings.randomize_bots {
            styles = BotStyle::ALL.to_vec();
            styles.shuffle(&mut rand::rng());
        }
        styles.truncate(settings.bot_count);
        styles
    }

    fn game_config(settings: &TableSettings) -> GameConfig {
        GameConfig {
            starting_stack: settings.starting_stack,
            small_blind: settings.small_blind,
            tournament_mode: settings.tournament_mode,
            hands_per_level: settings.hands_per_level,
        }
    }

    fn new_game(&mut self) {
        let styles = Self::resolved_styles(&self.settings);
        self.game = Game::with_bots(&styles, Self::game_config(&self.settings));
        self.raise_to = self.game.big_blind * 2;
        self.raise_cap = 1000.min(self.settings.starting_stack);
        self.raise_window_open = false;
        self.raise_all_in_limit = false;
        self.chip_motions.clear();
        self.community_reveals.clear();
        self.analysis_open = false;
        self.bot_due = Instant::now() + Duration::from_millis(650);
    }

    fn perform_action(&mut self, action: Action) {
        self.raise_window_open = false;
        let player = self.game.current;
        let old_stack = self.game.players[player].stack;
        let mut old_bets: Vec<u32> = self.game.players.iter().map(|p| p.street_bet).collect();
        let old_street = self.game.street;
        let old_community_len = self.game.community.len();
        let now = Instant::now();
        self.game.act(action);
        let paid = old_stack.saturating_sub(self.game.players[player].stack);
        if paid > 0 {
            self.chip_motions.push(ChipMotion {
                player,
                amount: paid,
                started: now,
                duration: Duration::from_millis(720),
                kind: ChipMotionKind::Bet,
            });
        }
        let gather_delay_ms = if paid > 0 { 760 } else { 250 };
        if old_street != self.game.street {
            old_bets[player] += paid;
            for (player, amount) in old_bets.into_iter().enumerate() {
                if amount > 0 {
                    self.chip_motions.push(ChipMotion {
                        player,
                        amount,
                        started: now + Duration::from_millis(gather_delay_ms),
                        duration: Duration::from_millis(780),
                        kind: ChipMotionKind::Gather,
                    });
                }
            }
        }
        let reveal_base_ms = gather_delay_ms + 820;
        let new_cards = self.game.community.len().saturating_sub(old_community_len);
        for (offset, index) in (old_community_len..self.game.community.len()).enumerate() {
            self.community_reveals.push(CommunityReveal {
                index,
                started: now + Duration::from_millis(reveal_base_ms + offset as u64 * 180),
                duration: Duration::from_millis(500),
            });
        }
        let award_delay_ms = reveal_base_ms
            + if new_cards > 0 {
                500 + (new_cards.saturating_sub(1) as u64) * 180
            } else {
                0
            };
        for (winner, &amount) in self.game.last_payouts.iter().enumerate() {
            if amount > 0 {
                self.chip_motions.push(ChipMotion {
                    player: winner,
                    amount,
                    started: now + Duration::from_millis(award_delay_ms),
                    duration: Duration::from_millis(980),
                    kind: ChipMotionKind::Award,
                });
            }
        }
        self.bot_due = now + Duration::from_millis(700);
    }

    fn card(
        ui: &mut egui::Ui,
        card: Option<Card>,
        size: Vec2,
        dimmed: bool,
        gold: bool,
        kicker: bool,
        flip_scale: f32,
    ) {
        let (full_rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let rect = Rect::from_center_size(
            full_rect.center(),
            Vec2::new(size.x * flip_scale.clamp(0.02, 1.0), size.y),
        );
        let painter = ui.painter();
        let face = if dimmed {
            Color32::from_rgb(112, 116, 114)
        } else if kicker {
            Color32::from_rgb(237, 221, 252)
        } else if gold {
            Color32::from_rgb(255, 246, 194)
        } else {
            Color32::from_rgb(245, 243, 235)
        };
        painter.rect(
            rect,
            7.0,
            if card.is_some() {
                face
            } else if dimmed {
                Color32::from_rgb(49, 55, 64)
            } else {
                Color32::from_rgb(34, 66, 112)
            },
            Stroke::new(
                if gold || kicker { 3.0 } else { 1.5 },
                if kicker {
                    Color32::from_rgb(166, 92, 224)
                } else if gold {
                    Color32::GOLD
                } else {
                    Color32::from_gray(190)
                },
            ),
            egui::StrokeKind::Inside,
        );
        if let Some(c) = card {
            let color = if dimmed {
                Color32::from_gray(63)
            } else if c.is_red() {
                Color32::from_rgb(190, 35, 42)
            } else {
                Color32::from_rgb(25, 28, 32)
            };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                format!("{}{}", c.rank_text(), c.suit_text()),
                FontId::proportional(size.y * 0.32),
                color,
            );
        } else {
            painter.circle_filled(
                rect.center(),
                size.x * 0.22,
                if dimmed {
                    Color32::from_gray(65)
                } else {
                    Color32::from_rgb(45, 94, 150)
                },
            );
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "♠",
                FontId::proportional(size.y * 0.25),
                if dimmed {
                    Color32::GRAY
                } else {
                    Color32::WHITE
                },
            );
        }
    }

    fn chip_color(denomination: u32) -> Color32 {
        match denomination {
            500 => Color32::from_rgb(28, 31, 36),
            100 => Color32::from_rgb(112, 58, 155),
            25 => Color32::from_rgb(43, 137, 74),
            5 => Color32::from_rgb(193, 48, 50),
            _ => Color32::from_rgb(230, 225, 209),
        }
    }

    fn chip_breakdown(mut amount: u32) -> Vec<(u32, usize, Color32)> {
        let mut result = Vec::new();
        for denomination in [500, 100, 25, 5, 1] {
            let count = amount / denomination;
            if count > 0 {
                result.push((denomination, count as usize, Self::chip_color(denomination)));
                amount %= denomination;
            }
        }
        result
    }

    fn paint_chip(painter: &egui::Painter, pos: Pos2, color: Color32, radius: f32) {
        let radii = Vec2::new(radius, radius * 0.46);
        let side = color.gamma_multiply(0.48);
        for depth in (1..=4).rev() {
            painter.add(egui::Shape::ellipse_filled(
                pos + Vec2::new(0.0, depth as f32),
                radii,
                side,
            ));
        }
        painter.add(egui::Shape::ellipse_filled(pos, radii, color));
        painter.add(egui::Shape::ellipse_stroke(
            pos,
            radii,
            Stroke::new(1.2, Color32::from_gray(225)),
        ));
        painter.add(egui::Shape::ellipse_stroke(
            pos,
            radii * 0.55,
            Stroke::new(1.0, Color32::from_black_alpha(100)),
        ));
    }

    fn paint_pile(painter: &egui::Painter, pos: Pos2, amount: u32, dimmed: bool) {
        if amount == 0 {
            return;
        }
        const MAX_TOWER_HEIGHT: usize = 8;
        let towers: Vec<(usize, Color32)> = Self::chip_breakdown(amount)
            .into_iter()
            .flat_map(|(_, mut count, color)| {
                let mut denomination_towers = Vec::new();
                while count > 0 {
                    let height = count.min(MAX_TOWER_HEIGHT);
                    denomination_towers.push((height, color));
                    count -= height;
                }
                denomination_towers
            })
            .collect();
        let width = (towers.len().saturating_sub(1) as f32) * 15.0;
        for (column, (count, base_color)) in towers.into_iter().enumerate() {
            let color = if dimmed {
                Color32::from_gray(75)
            } else {
                base_color
            };
            for index in 0..count {
                let chip_pos =
                    pos + Vec2::new(column as f32 * 15.0 - width / 2.0, -(index as f32) * 4.8);
                Self::paint_chip(painter, chip_pos, color, 9.0);
            }
        }
    }

    fn toward(from: Pos2, to: Pos2, distance: f32) -> Pos2 {
        let delta = to - from;
        if delta.length_sq() == 0.0 {
            from
        } else {
            from + delta.normalized() * distance
        }
    }

    fn table_animating(&self) -> bool {
        !self.chip_motions.is_empty() || !self.community_reveals.is_empty()
    }

    fn snap_raise(value: u32, minimum: u32, maximum: u32, step: u32) -> u32 {
        if minimum >= maximum {
            return maximum;
        }
        let value = value.clamp(minimum, maximum);
        let step = step.max(1);
        let offset = value - minimum;
        let snapped = minimum.saturating_add(((offset + step / 2) / step) * step);
        if maximum.saturating_sub(snapped.min(maximum)) < step.div_ceil(2) {
            maximum
        } else {
            snapped.clamp(minimum, maximum)
        }
    }

    fn seat_positions(center: Pos2, rx: f32, ry: f32) -> [Pos2; 5] {
        [
            Pos2::new(center.x, center.y + ry + 24.0),
            Pos2::new(center.x - rx + 55.0, center.y + 80.0),
            Pos2::new(center.x - rx + 145.0, center.y - ry + 8.0),
            Pos2::new(center.x + rx - 145.0, center.y - ry + 8.0),
            Pos2::new(center.x + rx - 55.0, center.y + 80.0),
        ]
    }

    fn seat(&self, ui: &mut egui::Ui, i: usize, pos: Pos2, human: bool) {
        let p = &self.game.players[i];
        let size = Vec2::new(174.0, if human { 116.0 } else { 100.0 });
        let rect = Rect::from_center_size(pos, size);
        let active = self.game.current == i && self.game.street != Street::Showdown;
        ui.painter().rect(
            rect,
            12.0,
            if p.eliminated {
                Color32::from_rgba_unmultiplied(30, 30, 30, 205)
            } else {
                Color32::from_rgba_unmultiplied(16, 23, 29, 235)
            },
            Stroke::new(
                if active { 3.0 } else { 1.0 },
                if active {
                    Color32::GOLD
                } else {
                    Color32::from_gray(85)
                },
            ),
            egui::StrokeKind::Outside,
        );
        let identity = if self.settings.hide_personalities {
            p.name.clone()
        } else {
            p.bot
                .map(|bot| format!("{} · {}", p.name, bot.label()))
                .unwrap_or_else(|| p.name.clone())
        };
        let title = if p.eliminated {
            format!("{} · выбыл", p.name)
        } else if p.folded {
            format!("{} · пас", p.name)
        } else {
            identity
        };
        ui.painter().text(
            pos + Vec2::new(0.0, -size.y / 2.0 + 16.0),
            Align2::CENTER_CENTER,
            title,
            FontId::proportional(17.0),
            if p.folded || p.eliminated {
                Color32::GRAY
            } else {
                Color32::WHITE
            },
        );
        ui.painter().text(
            pos + Vec2::new(0.0, size.y / 2.0 - 15.0),
            Align2::CENTER_CENTER,
            format!("{} фишек  ·  ставка {}", p.stack, p.street_bet),
            FontId::proportional(13.0),
            Color32::from_rgb(225, 190, 95),
        );
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(Rect::from_center_size(
                pos + Vec2::new(0.0, 3.0),
                Vec2::new(90.0, 52.0),
            )),
            |ui| {
                ui.horizontal_centered(|ui| {
                    for c in &p.hole {
                        let show = !p.eliminated && (human || self.game.reveal_all && !p.folded);
                        Self::card(
                            ui,
                            if show { Some(*c) } else { None },
                            Vec2::new(38.0, 50.0),
                            p.folded || p.eliminated,
                            self.game.highlighted_cards.contains(c)
                                && !self.game.kicker_cards.contains(c),
                            self.game.kicker_cards.contains(c),
                            1.0,
                        );
                    }
                });
            },
        );
        if i == self.game.dealer {
            ui.painter().circle_filled(
                pos + Vec2::new(size.x / 2.0 - 5.0, -size.y / 2.0 + 5.0),
                13.0,
                Color32::WHITE,
            );
            ui.painter().text(
                pos + Vec2::new(size.x / 2.0 - 5.0, -size.y / 2.0 + 5.0),
                Align2::CENTER_CENTER,
                "D",
                FontId::proportional(12.0),
                Color32::BLACK,
            );
        }
    }

    fn empty_seat(ui: &mut egui::Ui, pos: Pos2) {
        let rect = Rect::from_center_size(pos, Vec2::new(174.0, 100.0));
        ui.painter().rect(
            rect,
            12.0,
            Color32::from_rgba_unmultiplied(20, 24, 27, 190),
            Stroke::new(1.0, Color32::from_gray(61)),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect(
            Rect::from_center_size(pos + Vec2::new(0.0, 10.0), Vec2::new(105.0, 45.0)),
            18.0,
            Color32::from_rgb(46, 39, 35),
            Stroke::new(1.0, Color32::from_rgb(82, 67, 56)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            pos + Vec2::new(0.0, -29.0),
            Align2::CENTER_CENTER,
            "Свободное место",
            FontId::proportional(15.0),
            Color32::from_gray(112),
        );
    }

    fn paint_table_chips(&mut self, ui: &mut egui::Ui, center: Pos2, positions: &[Pos2]) {
        let painter = ui.painter().clone();
        let now = Instant::now();
        for (i, player) in self.game.players.iter().enumerate() {
            let direction = (center - positions[i]).normalized();
            let perpendicular = Vec2::new(-direction.y, direction.x);
            let bank_pos = Self::toward(positions[i], center, 75.0) + perpendicular * 31.0;
            let bet_pos = Self::toward(positions[i], center, 142.0);
            let pending_award: u32 = self
                .chip_motions
                .iter()
                .filter(|motion| {
                    matches!(motion.kind, ChipMotionKind::Award)
                        && motion.player == i
                        && now
                            .checked_duration_since(motion.started)
                            .is_none_or(|elapsed| elapsed < motion.duration)
                })
                .map(|motion| motion.amount)
                .sum();
            Self::paint_pile(
                &painter,
                bank_pos,
                player.stack.saturating_sub(pending_award),
                player.folded,
            );
            let waiting_to_gather: u32 = self
                .chip_motions
                .iter()
                .filter(|motion| {
                    matches!(motion.kind, ChipMotionKind::Gather)
                        && motion.player == i
                        && now.checked_duration_since(motion.started).is_none()
                })
                .map(|motion| motion.amount)
                .sum();
            let still_in_flight: u32 = self
                .chip_motions
                .iter()
                .filter(|motion| {
                    matches!(motion.kind, ChipMotionKind::Bet)
                        && motion.player == i
                        && now
                            .checked_duration_since(motion.started)
                            .is_none_or(|elapsed| elapsed < motion.duration)
                })
                .map(|motion| motion.amount)
                .sum();
            Self::paint_pile(
                &painter,
                bet_pos,
                (player.street_bet + waiting_to_gather).saturating_sub(still_in_flight),
                player.folded,
            );
        }

        let settled_pot = self
            .game
            .pot
            .saturating_sub(self.game.players.iter().map(|p| p.street_bet).sum());
        let pending_gather: u32 = self
            .chip_motions
            .iter()
            .filter(|motion| {
                matches!(motion.kind, ChipMotionKind::Gather)
                    && now
                        .checked_duration_since(motion.started)
                        .is_none_or(|elapsed| elapsed < motion.duration)
            })
            .map(|motion| motion.amount)
            .sum();
        let award_in_bank: u32 = self
            .chip_motions
            .iter()
            .filter(|motion| {
                matches!(motion.kind, ChipMotionKind::Award)
                    && now
                        .checked_duration_since(motion.started)
                        .is_some_and(|elapsed| elapsed < motion.duration)
            })
            .map(|motion| motion.amount)
            .sum();
        let pot_pos = center + Vec2::new(0.0, -78.0);
        Self::paint_pile(
            &painter,
            pot_pos,
            settled_pot.saturating_sub(pending_gather) + award_in_bank,
            false,
        );

        let mut animating = false;
        for motion in &self.chip_motions {
            let Some(elapsed) = now.checked_duration_since(motion.started) else {
                animating = true;
                continue;
            };
            let raw = elapsed.as_secs_f32() / motion.duration.as_secs_f32();
            if raw >= 1.0 {
                continue;
            }
            animating = true;
            let start = match motion.kind {
                ChipMotionKind::Bet => {
                    let direction = (center - positions[motion.player]).normalized();
                    let perpendicular = Vec2::new(-direction.y, direction.x);
                    Self::toward(positions[motion.player], center, 75.0) + perpendicular * 31.0
                }
                ChipMotionKind::Gather => Self::toward(positions[motion.player], center, 142.0),
                ChipMotionKind::Award => pot_pos,
            };
            let end = match motion.kind {
                ChipMotionKind::Bet => Self::toward(positions[motion.player], center, 142.0),
                ChipMotionKind::Gather => pot_pos,
                ChipMotionKind::Award => {
                    let direction = (center - positions[motion.player]).normalized();
                    let perpendicular = Vec2::new(-direction.y, direction.x);
                    Self::toward(positions[motion.player], center, 75.0) + perpendicular * 31.0
                }
            };
            let moving_colors: Vec<Color32> = Self::chip_breakdown(motion.amount)
                .into_iter()
                .flat_map(|(_, count, color)| std::iter::repeat_n(color, count))
                .take(8)
                .collect();
            for (chip, color) in moving_colors.into_iter().enumerate() {
                let delay = chip as f32 * 0.075;
                let t = ((raw - delay) / (1.0 - delay)).clamp(0.0, 1.0);
                if t <= 0.0 {
                    continue;
                }
                let smooth = t * t * (3.0 - 2.0 * t);
                let mut pos = start.lerp(end, smooth);
                if matches!(motion.kind, ChipMotionKind::Bet) {
                    pos.y -= (std::f32::consts::PI * t).sin() * (48.0 + chip as f32 * 4.0);
                }
                pos += Vec2::new((chip as f32 - 2.0) * 2.0, -(chip as f32) * 2.2);
                Self::paint_chip(&painter, pos, color, 9.0);
            }
        }
        self.chip_motions.retain(|motion| {
            now.checked_duration_since(motion.started)
                .is_none_or(|elapsed| elapsed < motion.duration)
        });
        if animating {
            ui.ctx().request_repaint();
        }
    }

    fn show_controls(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("controls")
            .exact_height(62.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(15, 20, 25))
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let reserved_for_controls = if self.game.street == Street::Showdown {
                        280.0
                    } else if self.game.is_human_turn() {
                        440.0
                    } else {
                        0.0
                    };
                    let message_width = (ui.available_width() - reserved_for_controls).max(80.0);
                    ui.add_sized(
                        [message_width, 38.0],
                        egui::Label::new(RichText::new(&self.game.message).size(16.0)).truncate(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.game.street == Street::Showdown {
                            let button_text = if self.table_animating() {
                                "Выплата банка…"
                            } else if self.game.game_over {
                                "Сыграть заново"
                            } else {
                                "Следующая раздача"
                            };
                            if ui
                                .add_enabled(
                                    !self.table_animating(),
                                    egui::Button::new(
                                        RichText::new(button_text).color(Color32::WHITE),
                                    )
                                    .fill(if self.table_animating() {
                                        Color32::from_rgb(70, 74, 76)
                                    } else {
                                        Color32::from_rgb(34, 139, 80)
                                    })
                                    .min_size(Vec2::new(160.0, 38.0)),
                                )
                                .clicked()
                            {
                                if self.game.game_over {
                                    self.new_game();
                                } else {
                                    self.game.start_hand();
                                    self.chip_motions.clear();
                                    self.community_reveals.clear();
                                    self.analysis_open = false;
                                    self.bot_due = Instant::now() + Duration::from_millis(600);
                                }
                            }
                            if !self.table_animating()
                                && !self.game.hand_analysis.is_empty()
                                && ui
                                    .add_sized(
                                        [72.0, 30.0],
                                        egui::Button::new(
                                            RichText::new("Разбор").color(Color32::BLACK),
                                        )
                                        .fill(Color32::from_rgb(234, 191, 84)),
                                    )
                                    .clicked()
                            {
                                self.analysis_open = true;
                            }
                        } else if self.game.is_human_turn() && !self.table_animating() {
                            let call = self.game.to_call(0);
                            let raise_bounds = self.game.raise_bounds(0);
                            if ui
                                .add_enabled(
                                    raise_bounds.is_some(),
                                    egui::Button::new(
                                        RichText::new("Повысить").color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(34, 139, 80))
                                    .min_size(Vec2::new(105.0, 38.0)),
                                )
                                .clicked()
                                && let Some((minimum, maximum)) = raise_bounds
                            {
                                self.raise_to = self.raise_to.clamp(minimum, maximum);
                                self.raise_window_open = true;
                            }
                            if ui
                                .add_sized(
                                    [125.0, 38.0],
                                    egui::Button::new(
                                        RichText::new(if call == 0 {
                                            "Чек"
                                        } else {
                                            "Уравнять"
                                        })
                                        .color(Color32::WHITE),
                                    )
                                    .fill(if call == 0 {
                                        Color32::from_rgb(42, 105, 190)
                                    } else {
                                        Color32::from_rgb(34, 139, 80)
                                    }),
                                )
                                .clicked()
                            {
                                self.perform_action(Action::CheckCall);
                            }
                            if ui
                                .add_sized(
                                    [90.0, 38.0],
                                    egui::Button::new(RichText::new("Пас").color(Color32::WHITE))
                                        .fill(Color32::from_rgb(181, 52, 57)),
                                )
                                .clicked()
                            {
                                self.perform_action(Action::Fold);
                            }
                        }
                    });
                });
            });
    }
}

impl eframe::App for PokerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.game.street != Street::Showdown
            && !self.game.is_human_turn()
            && !self.table_animating()
            && Instant::now() >= self.bot_due
        {
            let i = self.game.current;
            let a = bots::decide(&self.game, i);
            self.perform_action(a);
            ctx.request_repaint();
        } else if self.game.street != Street::Showdown && !self.game.is_human_turn() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        egui::TopBottomPanel::top("top")
            .exact_height(50.0)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(18, 23, 29))
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("OFFICE HOLD'EM").color(Color32::from_rgb(234, 191, 84)),
                    );
                    ui.separator();
                    ui.label(format!(
                        "{} игр. · {}/{} · уровень {} · раздача {}",
                        self.game.players.len(),
                        self.game.small_blind,
                        self.game.big_blind,
                        self.game.blind_level,
                        self.game.hand_number
                    ));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Новая игра").clicked() {
                            self.new_game();
                        }
                        if ui.button("⚙ Настройки").clicked() {
                            self.settings_open = true;
                        }
                    });
                });
            });
        self.show_controls(ctx);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::from_rgb(25, 34, 38)))
            .show(ctx, |ui| {
                let area = ui.available_rect_before_wrap();
                let center = Pos2::new(area.center().x, area.center().y - 20.0);
                let table_height = (area.height() - 140.0).clamp(280.0, 430.0);
                let table = Rect::from_center_size(
                    center,
                    Vec2::new((area.width() - 100.0).min(900.0), table_height),
                );
                ui.painter().rect(
                    table,
                    table.height() / 2.0,
                    Color32::from_rgb(18, 105, 73),
                    Stroke::new(10.0, Color32::from_rgb(87, 55, 33)),
                    egui::StrokeKind::Outside,
                );
                ui.painter().rect_stroke(
                    table.shrink(16.0),
                    table.height() / 2.0,
                    Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 255, 255, 38)),
                    egui::StrokeKind::Inside,
                );
                let rx = table.width() / 2.0;
                let ry = table.height() / 2.0;
                let positions = Self::seat_positions(center, rx, ry);
                for (i, &pos) in positions.iter().enumerate() {
                    if i < self.game.players.len() {
                        self.seat(ui, i, pos, i == 0);
                    } else {
                        Self::empty_seat(ui, pos);
                    }
                }
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(Rect::from_center_size(
                        center + Vec2::new(0.0, 8.0),
                        Vec2::new(430.0, 110.0),
                    )),
                    |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "БАНК  {}",
                                    self.game.pot.saturating_sub(
                                        self.game.players.iter().map(|p| p.street_bet).sum()
                                    )
                                ))
                                .size(19.0)
                                .color(Color32::from_rgb(255, 218, 120)),
                            );
                            ui.add_space(5.0);
                            ui.horizontal_centered(|ui| {
                                let now = Instant::now();
                                for n in 0..5 {
                                    let dealt = self.game.community.get(n).copied();
                                    let mut shown = dealt;
                                    let mut flip_scale = 1.0;
                                    if let Some(reveal) =
                                        self.community_reveals.iter().find(|r| r.index == n)
                                    {
                                        if let Some(elapsed) =
                                            now.checked_duration_since(reveal.started)
                                        {
                                            let progress = (elapsed.as_secs_f32()
                                                / reveal.duration.as_secs_f32())
                                            .clamp(0.0, 1.0);
                                            if progress < 0.5 {
                                                shown = None;
                                                flip_scale = 1.0 - progress * 2.0;
                                            } else {
                                                flip_scale = (progress - 0.5) * 2.0;
                                            }
                                        } else {
                                            shown = None;
                                        }
                                        ui.ctx().request_repaint();
                                    }
                                    let face_is_visible = shown.is_some();
                                    Self::card(
                                        ui,
                                        shown,
                                        Vec2::new(60.0, 82.0),
                                        false,
                                        face_is_visible
                                            && dealt.is_some_and(|c| {
                                                self.game.highlighted_cards.contains(&c)
                                                    && !self.game.kicker_cards.contains(&c)
                                            }),
                                        face_is_visible
                                            && dealt.is_some_and(|c| {
                                                self.game.kicker_cards.contains(&c)
                                            }),
                                        flip_scale,
                                    );
                                }
                                self.community_reveals.retain(|reveal| {
                                    now.checked_duration_since(reveal.started)
                                        .is_none_or(|elapsed| elapsed < reveal.duration)
                                });
                            });
                        });
                    },
                );
                self.paint_table_chips(ui, center, &positions);
            });

        if !self.game.is_human_turn() {
            self.raise_window_open = false;
        }
        let mut confirm_raise = false;
        let mut close_raise = false;
        if self.raise_window_open
            && let Some((minimum, all_in_total)) = self.game.raise_bounds(0)
        {
            let mut window_open = self.raise_window_open;
            egui::Window::new("Размер рейза")
                .open(&mut window_open)
                .anchor(Align2::CENTER_BOTTOM, Vec2::new(0.0, -76.0))
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.set_min_width(470.0);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(!self.raise_all_in_limit, "Макс")
                            .clicked()
                        {
                            self.raise_all_in_limit = false;
                        }
                        let cap_response = ui.add(
                            egui::DragValue::new(&mut self.raise_cap)
                                .range(1..=1_000_000)
                                .speed(self.game.small_blind),
                        );
                        if cap_response.changed() {
                            self.raise_all_in_limit = false;
                        }
                        let stack = self.game.players[0].stack;
                        if ui
                            .selectable_label(
                                self.raise_all_in_limit,
                                format!("Макс на руках [{stack}]"),
                            )
                            .clicked()
                        {
                            self.raise_all_in_limit = true;
                            self.raise_to = all_in_total;
                        }
                    });

                    let upper = if self.raise_all_in_limit {
                        all_in_total
                    } else {
                        self.raise_cap.clamp(minimum, all_in_total)
                    };
                    self.raise_to = Self::snap_raise(
                        self.raise_to.clamp(minimum, upper),
                        minimum,
                        upper,
                        self.game.small_blind,
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(format!("Минимум {minimum}"));
                        ui.add_space(8.0);
                        let response = ui.add(
                            egui::Slider::new(&mut self.raise_to, minimum..=upper)
                                .show_value(false)
                                .step_by(self.game.small_blind.max(1) as f64),
                        );
                        if response.changed() {
                            self.raise_to = Self::snap_raise(
                                self.raise_to,
                                minimum,
                                upper,
                                self.game.small_blind,
                            );
                        }
                        ui.label(format!("Верх {upper}"));
                    });
                    ui.vertical_centered(|ui| {
                        ui.heading(RichText::new(format!("Рейз до {}", self.raise_to)));
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "Малый блайнд: {} · шаг ползунка: {}",
                            self.game.small_blind, self.game.small_blind
                        ));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Подтвердить").color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(34, 139, 80)),
                                )
                                .clicked()
                            {
                                confirm_raise = true;
                            }
                            if ui.button("Отмена").clicked() {
                                close_raise = true;
                            }
                        });
                    });
                });
            self.raise_window_open = window_open && !close_raise;
        }
        if confirm_raise {
            self.perform_action(Action::Raise(self.raise_to));
        }

        let mut apply_settings = false;
        let mut settings_open = self.settings_open;
        egui::Window::new("Настройки стола")
            .open(&mut settings_open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(390.0);
                ui.checkbox(
                    &mut self.settings.hide_personalities,
                    "Скрывать характеры ботов",
                );
                ui.label(
                    RichText::new("Включено по умолчанию: за столом видны только имена.")
                        .small()
                        .color(Color32::GRAY),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Количество ботов:");
                    ui.add(egui::Slider::new(&mut self.settings.bot_count, 1..=4));
                });
                ui.label(
                    RichText::new(format!(
                        "За столом будет {} {}.",
                        self.settings.bot_count + 1,
                        if self.settings.bot_count == 1 {
                            "игрока"
                        } else {
                            "игроков"
                        }
                    ))
                    .small()
                    .color(Color32::GRAY),
                );
                ui.add_space(10.0);
                ui.separator();
                ui.label(RichText::new("Фишки и ставки").strong());
                ui.horizontal(|ui| {
                    ui.label("Стартовый капитал каждого:");
                    ui.add(
                        egui::DragValue::new(&mut self.settings.starting_stack)
                            .range(100..=1_000_000)
                            .speed(100),
                    );
                });
                let maximum_small_blind = (self.settings.starting_stack / 2).max(1);
                self.settings.small_blind = self.settings.small_blind.clamp(1, maximum_small_blind);
                ui.horizontal(|ui| {
                    ui.label("Начальные блайнды:");
                    ui.add(
                        egui::DragValue::new(&mut self.settings.small_blind)
                            .range(1..=maximum_small_blind)
                            .speed(5),
                    );
                    ui.label(format!("/ {}", self.settings.small_blind * 2));
                });
                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.settings.tournament_mode,
                    "Турнирный режим: блайнды удваиваются",
                );
                if self.settings.tournament_mode {
                    ui.horizontal(|ui| {
                        ui.label("Раздач на одном уровне:");
                        ui.add(
                            egui::DragValue::new(&mut self.settings.hands_per_level).range(1..=100),
                        );
                    });
                    ui.label(
                        RichText::new("После указанного числа раздач малый и большой блайнды ×2.")
                            .small()
                            .color(Color32::GRAY),
                    );
                }
                ui.add_space(10.0);
                ui.separator();
                ui.checkbox(
                    &mut self.settings.randomize_bots,
                    "Случайный состав при новой игре",
                );
                if self.settings.randomize_bots {
                    ui.label("Архетипы тайно выбираются для заданного числа соперников.");
                } else {
                    ui.add_space(6.0);
                    let bot_names = ["Марина", "Борис", "Лис", "Профессор"];
                    for (seat, bot_name) in
                        bot_names.iter().enumerate().take(self.settings.bot_count)
                    {
                        ui.horizontal(|ui| {
                            ui.label(format!("{bot_name}:"));
                            egui::ComboBox::from_id_salt(("bot_style", seat))
                                .selected_text(self.settings.selected[seat].label())
                                .show_ui(ui, |ui| {
                                    for style in BotStyle::ALL {
                                        ui.selectable_value(
                                            &mut self.settings.selected[seat],
                                            style,
                                            style.label(),
                                        );
                                    }
                                });
                        });
                    }
                }
                ui.add_space(12.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Применить и начать новую игру").clicked()
                    {
                        apply_settings = true;
                    }
                    ui.label(
                        RichText::new("Текущая раздача будет заменена")
                            .small()
                            .color(Color32::GRAY),
                    );
                });
            });
        self.settings_open = settings_open;
        if apply_settings {
            self.new_game();
            self.settings_open = false;
        }

        if self.analysis_open && !self.table_animating() && !self.game.hand_analysis.is_empty() {
            let mut analysis_open = self.analysis_open;
            egui::Window::new("Разбор раздачи")
                .open(&mut analysis_open)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.set_max_width(560.0);
                    ui.set_min_width(500.0);
                    for (index, line) in self.game.hand_analysis.iter().enumerate() {
                        if index > 0 {
                            ui.add_space(7.0);
                        }
                        ui.label(RichText::new(line).size(16.0));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Золотой").color(Color32::GOLD));
                        ui.label("— основная комбинация;");
                        ui.label(
                            RichText::new("фиолетовый").color(Color32::from_rgb(183, 112, 235)),
                        );
                        ui.label("— кикеры.");
                    });
                });
            self.analysis_open = analysis_open;
        }

        if self.game.game_over && !self.table_animating() {
            let mut restart = false;
            egui::Window::new(if self.game.human_won {
                "Победа!"
            } else {
                "Игра окончена"
            })
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_width(330.0);
                ui.vertical_centered(|ui| {
                    ui.heading(if self.game.human_won {
                        "Вы выиграли турнир"
                    } else {
                        "Ваши фишки закончились"
                    });
                    ui.add_space(8.0);
                    ui.label(if self.game.human_won {
                        "Все боты выбыли из игры."
                    } else {
                        "Вы выбыли из игры. Хотите начать с новым стеком?"
                    });
                    for line in &self.game.hand_analysis {
                        ui.add_space(6.0);
                        ui.label(line);
                    }
                    ui.add_space(14.0);
                    if ui
                        .add_sized([180.0, 40.0], egui::Button::new("Сыграть заново"))
                        .clicked()
                    {
                        restart = true;
                    }
                });
            });
            if restart {
                self.new_game();
            }
        }
    }
}

mod info;

use std::collections::HashMap;
use std::sync::Arc;

use egui::{
    Align2, Color32, CornerRadius, FontId, Galley, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use super::{
    ACCENT, chrome_alpha, clean_display_name, content_color, content_gray, elided_galley,
    rgba_alpha, scale_color_alpha, text, wheel_units,
};
use crate::app::TextKey;
use crate::overlay_protocol::{FromOverlay, ModHotkey, ModHotkeys, ModInfo};

const FLIP_SECS: f64 = 0.24;
const BUTTON_SIZE: f32 = 21.0;
const BUTTON_GAP: f32 = 4.0;
const LABEL_SECS: f32 = 0.12;
const LABEL_PADDING: f32 = 5.0;
const LABEL_GAP: f32 = 2.0;
const INSET: f32 = 8.0;
const PADDING: f32 = 10.0;
const HEADER_HEIGHT: f32 = 37.0;
const ROW_HEIGHT: f32 = 24.0;
const KEYCAP_HEIGHT: f32 = 18.0;
const LIT_SECS: f64 = 0.6;
const ANSWER_SECS: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Keys,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Face {
    pub back: bool,
    pub width: f32,
}

impl Face {
    pub(super) const FRONT: Self = Self {
        back: false,
        width: 1.0,
    };

    pub(super) fn settled(self) -> bool {
        self.width >= 1.0
    }
}

pub(super) struct Buttons {
    list: Vec<BackButton>,
}

impl Buttons {
    pub(super) fn open(&self) -> f32 {
        self.list
            .iter()
            .map(|button| button.open)
            .fold(0.0, f32::max)
    }

    pub(super) fn room(&self) -> f32 {
        self.list.len() as f32 * BUTTON_ROOM
    }

    fn left(&self) -> Option<f32> {
        self.list.last().map(|button| button.rect.min.x)
    }
}

struct BackButton {
    side: Side,
    id: egui::Id,
    lit: bool,
    rect: Rect,
    label: Arc<Galley>,
    open: f32,
}

#[derive(Default)]
struct Label {
    id: Option<egui::Id>,
    open: f32,
    pass: u64,
    held: bool,
}

impl Label {
    fn shown(&self, id: egui::Id, pass: u64) -> f32 {
        if self.id == Some(id) && self.pass + 1 >= pass {
            egui::emath::easing::cubic_out(self.open)
        } else {
            0.0
        }
    }

    fn update(&mut self, id: egui::Id, hovered: bool, pass: u64, dt: f32) -> f32 {
        if self.id != Some(id) || self.pass + 1 < pass {
            self.open = 0.0;
        }
        self.id = Some(id);
        self.pass = pass;
        self.held &= hovered;
        let step = dt / LABEL_SECS;
        self.open = if hovered && !self.held {
            (self.open + step).min(1.0)
        } else {
            (self.open - step).max(0.0)
        };
        egui::emath::easing::cubic_out(self.open)
    }

    fn moving(&self) -> bool {
        self.open > 0.0 && self.open < 1.0
    }

    fn hold(&mut self) {
        self.held = true;
        self.open = 0.0;
    }
}

struct Flip {
    mod_id: String,
    back: bool,
    side: Side,
    started_at: f64,
    scroll: usize,
    selected: usize,
}

impl Flip {
    fn turn(&self, now: f64) -> f32 {
        let progress = ((now - self.started_at) / FLIP_SECS).clamp(0.0, 1.0) as f32;
        if self.back { progress } else { 1.0 - progress }
    }

    fn turning(&self, now: f64) -> bool {
        now - self.started_at < FLIP_SECS
    }
}

struct Press {
    id: u64,
    mod_id: String,
    index: usize,
    sent_at: f64,
}

struct Lit {
    mod_id: String,
    index: usize,
    at: f64,
}

#[derive(Default)]
pub(super) struct CardKeys {
    hotkeys: HashMap<String, Vec<ModHotkey>>,
    info: info::CardInfo,
    asked: HashMap<String, u64>,
    library: u64,
    requests: Vec<FromOverlay>,
    flip: Option<Flip>,
    own_clicks: Vec<Rect>,
    last_own_clicks: Vec<Rect>,
    back: Option<Rect>,
    last_back: Option<Rect>,
    hidden_rows: usize,
    fit: usize,
    wheel: f32,
    labels: [Label; 2],
    presses: Vec<Press>,
    next_press: u64,
    lit: Option<Lit>,
}

impl CardKeys {
    pub(super) fn ask(&mut self, mod_id: &str) {
        if self.asked.get(mod_id) != Some(&self.library) {
            self.asked.insert(mod_id.to_owned(), self.library);
            self.requests.push(FromOverlay::Hotkeys {
                mod_id: mod_id.to_owned(),
            });
            self.requests.push(FromOverlay::Info {
                mod_id: mod_id.to_owned(),
            });
        }
    }

    pub(super) fn take_requests(&mut self) -> Vec<FromOverlay> {
        std::mem::take(&mut self.requests)
    }

    pub(super) fn receive(&mut self, answer: ModHotkeys) {
        if let Some(flip) = self
            .flip
            .as_mut()
            .filter(|flip| flip.mod_id == answer.mod_id)
        {
            flip.selected = flip.selected.min(answer.hotkeys.len().saturating_sub(1));
        }
        self.hotkeys.insert(answer.mod_id, answer.hotkeys);
    }

    pub(super) fn receive_info(&mut self, answer: ModInfo) {
        self.info.receive(answer);
    }

    pub(super) fn library_changed(&mut self) {
        self.library += 1;
    }

    pub(super) fn has(&self, mod_id: &str) -> bool {
        self.hotkeys
            .get(mod_id)
            .is_some_and(|list| !list.is_empty())
    }

    pub(super) fn has_info(&self, mod_id: &str) -> bool {
        self.info.has(mod_id)
    }

    fn has_side(&self, mod_id: &str, side: Side) -> bool {
        match side {
            Side::Keys => self.has(mod_id),
            Side::Info => self.has_info(mod_id),
        }
    }

    pub(super) fn toggle(&mut self, mod_id: &str, side: Side, now: f64) {
        let has = self.has_side(mod_id, side);
        match &mut self.flip {
            Some(flip) if flip.mod_id == mod_id && flip.back && flip.side != side => {
                if !has {
                    return;
                }
                flip.side = side;
                flip.scroll = 0;
                flip.selected = 0;
            }
            Some(flip) if flip.mod_id == mod_id => {
                if !flip.back {
                    if !has {
                        return;
                    }
                    if flip.side != side {
                        flip.side = side;
                        flip.scroll = 0;
                        flip.selected = 0;
                    }
                }
                let turn = flip.turn(now);
                flip.back = !flip.back;
                let remaining = if flip.back { 1.0 - turn } else { turn };
                flip.started_at = now - f64::from(1.0 - remaining) * FLIP_SECS;
            }
            _ if has => {
                self.flip = Some(Flip {
                    mod_id: mod_id.to_owned(),
                    back: true,
                    side,
                    started_at: now,
                    scroll: 0,
                    selected: 0,
                });
            }
            _ => return,
        }
        self.info.close_menu();
        if side == Side::Info && self.showing_info(mod_id) {
            self.requests.push(FromOverlay::Info {
                mod_id: mod_id.to_owned(),
            });
        }
        self.labels[side as usize].hold();
    }

    pub(super) fn reset(&mut self) {
        self.flip = None;
        self.back = None;
        self.last_back = None;
        self.info.close_menu();
    }

    pub(super) fn keep_focused(&mut self, focused: Option<&str>) {
        if self
            .flip
            .as_ref()
            .is_some_and(|flip| Some(flip.mod_id.as_str()) != focused)
        {
            self.reset();
        }
    }

    pub(super) fn face(&mut self, mod_id: &str, now: f64) -> Face {
        let Some(flip) = self.flip.as_ref().filter(|flip| flip.mod_id == mod_id) else {
            return Face::FRONT;
        };
        let turn = flip.turn(now);
        if !flip.back && turn <= 0.0 {
            self.reset();
            return Face::FRONT;
        }
        let angle = turn * std::f32::consts::PI;
        Face {
            back: turn >= 0.5,
            width: angle.cos().abs(),
        }
    }

    pub(super) fn turning(&self, now: f64) -> bool {
        self.flip.as_ref().is_some_and(|flip| flip.turning(now))
    }

    pub(super) fn side(&self) -> Side {
        self.flip.as_ref().map_or(Side::Keys, |flip| flip.side)
    }

    pub(super) fn showing(&self, mod_id: &str) -> bool {
        self.showing_side(mod_id, Side::Keys)
    }

    pub(super) fn showing_info(&self, mod_id: &str) -> bool {
        self.showing_side(mod_id, Side::Info)
    }

    fn showing_side(&self, mod_id: &str, side: Side) -> bool {
        self.flip
            .as_ref()
            .is_some_and(|flip| flip.mod_id == mod_id && flip.back && flip.side == side)
    }

    pub(super) fn select(&mut self, direction: i32) -> bool {
        let Some(flip) = self.flip.as_mut().filter(|flip| flip.back) else {
            return false;
        };
        if flip.side == Side::Info {
            let scroll = if direction > 0 {
                (flip.scroll + info::STEP_LINES).min(self.hidden_rows)
            } else {
                flip.scroll.saturating_sub(info::STEP_LINES)
            };
            let moved = scroll != flip.scroll;
            flip.scroll = scroll;
            return moved;
        }
        let count = self.hotkeys.get(&flip.mod_id).map_or(0, Vec::len);
        let Some(selected) = flip
            .selected
            .checked_add_signed(direction.signum() as isize)
            .filter(|&selected| selected < count)
        else {
            return false;
        };
        flip.selected = selected;
        flip.scroll = flip
            .scroll
            .min(selected)
            .max((selected + 1).saturating_sub(self.fit.max(1)));
        true
    }

    pub(super) fn press_selected(&mut self, now: f64) {
        if let Some(flip) = self
            .flip
            .as_ref()
            .filter(|flip| flip.back && flip.side == Side::Keys)
        {
            let (mod_id, index) = (flip.mod_id.clone(), flip.selected);
            self.press(mod_id, index, now);
        }
    }

    fn press(&mut self, mod_id: String, index: usize, now: f64) {
        let Some(row) = self
            .hotkeys
            .get(&mod_id)
            .and_then(|rows| rows.get(index))
            .filter(|row| !row.unavailable)
        else {
            return;
        };
        let key = row.raw.clone();
        self.next_press += 1;
        let id = self.next_press;
        self.presses
            .retain(|press| now - press.sent_at < ANSWER_SECS);
        self.presses.push(Press {
            id,
            mod_id: mod_id.clone(),
            index,
            sent_at: now,
        });
        self.requests.push(FromOverlay::PressHotkey {
            id,
            mod_id,
            index,
            key,
        });
    }

    pub(super) fn pressed(&mut self, id: u64, error: Option<String>, now: f64) -> Option<String> {
        if let Some(at) = self.presses.iter().position(|press| press.id == id) {
            let press = self.presses.remove(at);
            if error.is_none() {
                self.lit = Some(Lit {
                    mod_id: press.mod_id,
                    index: press.index,
                    at: now,
                });
            }
        }
        error
    }

    pub(super) fn begin_frame(&mut self) {
        self.last_own_clicks = std::mem::take(&mut self.own_clicks);
        self.last_back = self.back.take();
    }

    pub(super) fn owns_press(&self, pos: egui::Pos2) -> bool {
        self.last_own_clicks.iter().any(|rect| rect.contains(pos))
    }

    pub(super) fn scroll_with_wheel(&mut self, ui: &mut Ui) {
        let (Some(flip), Some(back)) = (&mut self.flip, self.last_back) else {
            return;
        };
        if self.hidden_rows == 0 || !ui.rect_contains_pointer(back) {
            return;
        }
        let mut steps = 0.0;
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    phase,
                } if !modifiers.ctrl && !modifiers.command => {
                    if *phase == egui::TouchPhase::Move {
                        steps += wheel_units(*unit, delta.y);
                    }
                    false
                }
                _ => true,
            });
            input
                .raw
                .events
                .retain(|event| !matches!(event, egui::Event::MouseWheel { .. }));
            input.smooth_scroll_delta = Vec2::ZERO;
        });
        let step = match flip.side {
            Side::Keys => 1,
            Side::Info => info::STEP_LINES,
        };
        self.wheel += steps;
        while self.wheel >= 1.0 {
            self.wheel -= 1.0;
            flip.scroll = flip.scroll.saturating_sub(step);
        }
        while self.wheel <= -1.0 {
            self.wheel += 1.0;
            flip.scroll = (flip.scroll + step).min(self.hidden_rows);
        }
    }

    pub(super) fn buttons(
        &mut self,
        ui: &Ui,
        card: Rect,
        id: egui::Id,
        mod_id: &str,
        back: bool,
        overlay_opacity: u8,
    ) -> Option<Buttons> {
        let lit = self.flip.as_ref().filter(|_| back).map(|flip| flip.side);
        let sides: Vec<Side> = [Side::Keys, Side::Info]
            .into_iter()
            .filter(|&side| self.has_side(mod_id, side))
            .collect();
        if sides.is_empty() {
            return None;
        }
        let pass = ui.ctx().cumulative_pass_nr();
        let dt = ui.input(|input| input.stable_dt).min(0.1);
        let mut right = button_rect(card).max.x;
        let mut list = Vec::with_capacity(sides.len());
        for (index, &side) in sides.iter().enumerate() {
            let lit = lit == Some(side);
            let (id, key) = match side {
                Side::Keys => (
                    id.with("keys"),
                    if lit {
                        TextKey::GameOverlayHideHotkeys
                    } else {
                        TextKey::GameOverlayShowHotkeys
                    },
                ),
                Side::Info => (
                    id.with("info"),
                    if lit {
                        TextKey::GameOverlayHideInfo
                    } else {
                        TextKey::GameOverlayShowInfo
                    },
                ),
            };
            let others = (sides.len() - index - 1) as f32 * BUTTON_ROOM;
            let label = elided_galley(
                ui,
                text(key),
                FontId::proportional(11.0),
                content_gray(235, overlay_opacity),
                right
                    - card.min.x
                    - INSET
                    - BUTTON_ROOM
                    - others
                    - BUTTON_SIZE
                    - LABEL_GAP
                    - LABEL_PADDING,
            );
            let open_width = BUTTON_SIZE + LABEL_GAP + label.size().x + LABEL_PADDING;
            let place = |open: f32| {
                let width = egui::lerp(BUTTON_SIZE..=open_width, open);
                Rect::from_min_max(
                    egui::pos2(right - width, card.min.y + INSET),
                    egui::pos2(right, card.min.y + INSET + BUTTON_SIZE),
                )
            };
            let state = &mut self.labels[side as usize];
            let hovered = ui.rect_contains_pointer(place(state.shown(id, pass)));
            let open = state.update(id, hovered, pass, dt);
            if state.moving() {
                ui.ctx().request_repaint();
            }
            let rect = place(open);
            right = rect.min.x - BUTTON_GAP;
            list.push(BackButton {
                side,
                id,
                lit,
                rect,
                label,
                open,
            });
        }
        Some(Buttons { list })
    }

    pub(super) fn paint_buttons(
        &mut self,
        ui: &Ui,
        buttons: Buttons,
        overlay_opacity: u8,
    ) -> Option<Side> {
        let mut clicked = None;
        for button in buttons.list {
            let side = button.side;
            if self.paint_button(ui, button, overlay_opacity) {
                clicked = Some(side);
            }
        }
        clicked
    }

    fn paint_button(&mut self, ui: &Ui, button: BackButton, overlay_opacity: u8) -> bool {
        let BackButton {
            side,
            id,
            lit,
            rect,
            label,
            open,
        } = button;
        self.own_clicks.push(rect);
        let response = ui
            .interact(rect, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = response.hovered();
        let fill = Color32::from_black_alpha(chrome_alpha(
            if hovered { 225 } else { 185 },
            overlay_opacity,
        ));
        ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
        let color = if lit {
            content_color(ACCENT, overlay_opacity)
        } else {
            content_gray(if hovered { 255 } else { 225 }, overlay_opacity)
        };
        let icon = Rect::from_min_max(egui::pos2(rect.max.x - BUTTON_SIZE, rect.min.y), rect.max);
        ui.painter().text(
            icon.center(),
            Align2::CENTER_CENTER,
            char::from(match side {
                Side::Keys => lucide_icons::Icon::Keyboard,
                Side::Info => lucide_icons::Icon::Info,
            })
            .to_string(),
            FontId::new(13.0, egui::FontFamily::Name("preview-icons".into())),
            color,
        );
        if open > 0.0 {
            let pos = egui::pos2(
                icon.min.x - LABEL_GAP - label.size().x,
                rect.center().y - label.size().y * 0.5,
            );
            ui.painter()
                .with_clip_rect(rect)
                .galley_with_override_text_color(
                    pos,
                    label,
                    scale_color_alpha(content_gray(235, overlay_opacity), open),
                );
        }
        response.clicked()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_back(
        &mut self,
        ui: &Ui,
        id: egui::Id,
        card: Rect,
        shown: Rect,
        mod_id: &str,
        name: &str,
        border: Stroke,
        reveal: f32,
        overlay_opacity: u8,
        selecting: bool,
        buttons: Option<Buttons>,
    ) -> Option<Side> {
        let settled = shown.width() >= card.width();
        if settled {
            self.own_clicks.push(card);
            self.back = Some(card);
        }
        let painter = ui.painter().with_clip_rect(shown);
        let fade = |color: Color32| scale_color_alpha(color, reveal);
        painter.rect_filled(
            shown,
            CornerRadius::ZERO,
            fade(rgba_alpha(24, 24, 24, 242, overlay_opacity)),
        );
        let inner = card.shrink(PADDING);
        let header = Rect::from_min_max(
            egui::pos2(inner.min.x, card.min.y + INSET),
            egui::pos2(
                buttons
                    .as_ref()
                    .and_then(Buttons::left)
                    .unwrap_or(button_rect(card).min.x)
                    - 8.0,
                card.min.y + INSET + BUTTON_SIZE,
            ),
        );
        let divider_y = card.min.y + HEADER_HEIGHT;
        painter.line_segment(
            [
                egui::pos2(inner.min.x, divider_y),
                egui::pos2(inner.max.x, divider_y),
            ],
            Stroke::new(1.0, fade(content_gray(68, overlay_opacity))),
        );
        let body = Rect::from_min_max(egui::pos2(inner.min.x, divider_y + 6.0), inner.max);
        let side = self.flip.as_ref().map_or(Side::Keys, |flip| flip.side);
        match side {
            Side::Keys => {
                let title = elided_galley(
                    ui,
                    &clean_display_name(name),
                    FontId::proportional(13.0),
                    fade(content_gray(240, overlay_opacity)),
                    header.width(),
                );
                painter.galley(
                    egui::pos2(header.min.x, header.center().y - title.size().y * 0.5),
                    title,
                    Color32::PLACEHOLDER,
                );
                self.paint_key_rows(
                    ui,
                    &painter,
                    id,
                    body,
                    mod_id,
                    settled,
                    reveal,
                    overlay_opacity,
                    selecting,
                );
            }
            Side::Info => {
                self.paint_info(
                    ui,
                    &painter,
                    id,
                    header,
                    body,
                    mod_id,
                    settled,
                    reveal,
                    overlay_opacity,
                );
            }
        }
        painter.rect_stroke(
            shown.shrink(0.5),
            CornerRadius::ZERO,
            Stroke::new(border.width, fade(border.color)),
            StrokeKind::Inside,
        );
        let clicked = buttons.and_then(|buttons| self.paint_buttons(ui, buttons, overlay_opacity));
        if side == Side::Info && settled {
            self.paint_info_menu(ui, id, card, header, mod_id, overlay_opacity);
        }
        clicked
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_key_rows(
        &mut self,
        ui: &Ui,
        painter: &egui::Painter,
        id: egui::Id,
        list: Rect,
        mod_id: &str,
        settled: bool,
        reveal: f32,
        overlay_opacity: u8,
        selecting: bool,
    ) {
        let now = ui.input(|input| input.time);
        let fade = |color: Color32| scale_color_alpha(color, reveal);
        let rows = self
            .hotkeys
            .get(mod_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let fit = ((list.height() / ROW_HEIGHT).floor() as usize).max(1);
        let hidden = rows.len().saturating_sub(fit);
        if settled {
            self.hidden_rows = hidden;
            self.fit = fit;
        }
        let (first, selected) = self.flip.as_mut().map_or((0, None), |flip| {
            flip.scroll = flip.scroll.min(hidden);
            (flip.scroll, selecting.then_some(flip.selected))
        });
        let mut clicked_row = None;
        let mut lit_moving = false;
        let key_font = FontId::proportional(11.0);
        let label_font = FontId::proportional(12.0);
        let key_color = fade(content_gray(235, overlay_opacity));
        let room = list.width() - if hidden > 0 { 6.0 } else { 0.0 };
        let column = rows
            .iter()
            .map(|row| {
                ui.painter()
                    .layout_no_wrap(row.key.clone(), key_font.clone(), key_color)
                    .size()
                    .x
                    + 10.0
            })
            .fold(0.0_f32, f32::max)
            .min(room * 0.5);
        for (line, row) in rows.iter().skip(first).take(fit).enumerate() {
            let index = first + line;
            let y = list.min.y + line as f32 * ROW_HEIGHT + ROW_HEIGHT * 0.5;
            let row_rect = Rect::from_min_max(
                egui::pos2(list.min.x - 4.0, y - ROW_HEIGHT * 0.5),
                egui::pos2(list.min.x + room, y + ROW_HEIGHT * 0.5),
            );
            let hovered = settled && !row.unavailable && {
                let response = ui
                    .interact(row_rect, id.with(index), Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() {
                    clicked_row = Some(index);
                }
                response.hovered()
            };
            if hovered || selected == Some(index) {
                painter.rect_filled(
                    row_rect,
                    CornerRadius::same(4),
                    fade(rgba_alpha(
                        255,
                        255,
                        255,
                        if hovered { 26 } else { 16 },
                        overlay_opacity,
                    )),
                );
            }
            let waiting = self.presses.iter().any(|press| {
                press.mod_id == mod_id && press.index == index && now - press.sent_at < ANSWER_SECS
            });
            let lit = self
                .lit
                .as_ref()
                .filter(|lit| lit.mod_id == mod_id && lit.index == index)
                .map(|lit| (now - lit.at) / LIT_SECS)
                .filter(|progress| (0.0..1.0).contains(progress))
                .map_or(0.0, |progress| 1.0 - progress as f32);
            lit_moving |= lit > 0.0;
            let grey = |color: Color32| {
                if row.unavailable {
                    scale_color_alpha(color, 0.4)
                } else {
                    color
                }
            };
            let key = elided_galley(
                ui,
                &row.key,
                key_font.clone(),
                grey(key_color),
                column - 10.0,
            );
            let keycap = Rect::from_min_size(
                egui::pos2(list.min.x, y - KEYCAP_HEIGHT * 0.5),
                Vec2::new(key.size().x + 10.0, KEYCAP_HEIGHT),
            );
            painter.rect_filled(
                keycap,
                CornerRadius::same(3),
                grey(fade(rgba_alpha(58, 58, 58, 255, overlay_opacity))),
            );
            if lit > 0.0 {
                painter.rect_filled(
                    keycap,
                    CornerRadius::same(3),
                    fade(scale_color_alpha(
                        content_color(ACCENT, overlay_opacity),
                        lit * 0.6,
                    )),
                );
            }
            painter.rect_stroke(
                keycap,
                CornerRadius::same(3),
                Stroke::new(
                    1.0,
                    grey(fade(if waiting || lit > 0.0 {
                        content_color(ACCENT, overlay_opacity)
                    } else {
                        content_gray(96, overlay_opacity)
                    })),
                ),
                StrokeKind::Inside,
            );
            painter.galley(
                keycap.center() - key.size() * 0.5,
                key,
                Color32::PLACEHOLDER,
            );
            let label = elided_galley(
                ui,
                &row.label,
                label_font.clone(),
                grey(fade(content_gray(214, overlay_opacity))),
                room - column - 8.0,
            );
            painter.galley(
                egui::pos2(list.min.x + column + 8.0, y - label.size().y * 0.5),
                label,
                Color32::PLACEHOLDER,
            );
        }
        if hidden > 0 {
            paint_scrollbar(
                painter,
                list,
                fit as f32 * ROW_HEIGHT,
                fit,
                rows.len(),
                first,
                reveal,
                overlay_opacity,
            );
        }
        if lit_moving {
            ui.ctx().request_repaint();
        }
        if let Some(index) = clicked_row {
            if let Some(flip) = &mut self.flip {
                flip.selected = index;
            }
            self.press(mod_id.to_owned(), index, now);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_scrollbar(
    painter: &egui::Painter,
    list: Rect,
    height: f32,
    fit: usize,
    count: usize,
    first: usize,
    reveal: f32,
    overlay_opacity: u8,
) {
    let hidden = count.saturating_sub(fit).max(1);
    let fade = |color: Color32| scale_color_alpha(color, reveal);
    let track = Rect::from_min_max(
        egui::pos2(list.max.x - 2.0, list.min.y),
        egui::pos2(list.max.x, list.min.y + height),
    );
    let share = fit as f32 / count.max(1) as f32;
    let thumb_height = (track.height() * share).max(12.0);
    let offset = (track.height() - thumb_height) * first.min(hidden) as f32 / hidden as f32;
    painter.rect_filled(
        track,
        CornerRadius::same(1),
        fade(content_gray(52, overlay_opacity)),
    );
    painter.rect_filled(
        Rect::from_min_size(
            egui::pos2(track.min.x, track.min.y + offset),
            Vec2::new(track.width(), thumb_height),
        ),
        CornerRadius::same(1),
        fade(content_gray(150, overlay_opacity)),
    );
}

pub(super) fn button_rect(card: Rect) -> Rect {
    Rect::from_min_size(
        egui::pos2(card.max.x - INSET - BUTTON_SIZE, card.min.y + INSET),
        Vec2::splat(BUTTON_SIZE),
    )
}

const BUTTON_ROOM: f32 = BUTTON_SIZE + BUTTON_GAP;

pub(super) fn narrowed(card: Rect, width: f32) -> Rect {
    Rect::from_center_size(
        card.center(),
        Vec2::new(card.width() * width.clamp(0.0, 1.0), card.height()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(mod_id: &str, count: usize) -> ModHotkeys {
        ModHotkeys {
            mod_id: mod_id.into(),
            hotkeys: (0..count)
                .map(|index| ModHotkey {
                    key: format!("F{index}"),
                    label: format!("Key {index}"),
                    raw: format!("VK_F{index}"),
                    unavailable: false,
                })
                .collect(),
        }
    }

    #[test]
    fn a_turn_keeps_the_label_in_until_the_pointer_leaves() {
        let id = egui::Id::new("keys");
        let mut label = Label::default();
        assert_eq!(label.update(id, true, 1, LABEL_SECS), 1.0);
        label.hold();
        assert_eq!(label.update(id, true, 20, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, true, 21, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, false, 22, LABEL_SECS), 0.0);
        assert_eq!(label.update(id, true, 23, LABEL_SECS), 1.0);
    }

    #[test]
    fn a_button_that_was_away_starts_with_its_label_in() {
        let id = egui::Id::new("keys");
        let mut label = Label::default();
        label.update(id, true, 1, LABEL_SECS);
        assert_eq!(label.shown(id, 2), 1.0);
        assert_eq!(label.shown(id, 3), 0.0);
        assert_eq!(label.shown(egui::Id::new("other"), 2), 0.0);
        assert!(label.update(id, true, 5, LABEL_SECS * 0.5) < 1.0);
    }

    #[test]
    fn turning_holds_the_label_but_a_card_without_hotkeys_does_not() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 2));
        keys.toggle("none", Side::Keys, 0.0);
        assert!(!keys.labels[Side::Keys as usize].held);
        keys.toggle("some", Side::Keys, 0.0);
        assert!(keys.labels[Side::Keys as usize].held);
    }

    #[test]
    fn asks_once_per_library() {
        let mut keys = CardKeys::default();
        keys.ask("a");
        keys.ask("a");
        assert_eq!(
            keys.take_requests(),
            vec![
                FromOverlay::Hotkeys { mod_id: "a".into() },
                FromOverlay::Info { mod_id: "a".into() },
            ]
        );
        keys.ask("a");
        assert!(keys.take_requests().is_empty());
        keys.library_changed();
        keys.ask("a");
        assert_eq!(keys.take_requests().len(), 2);
    }

    fn info(mod_id: &str) -> ModInfo {
        ModInfo {
            mod_id: mod_id.into(),
            sources: vec![crate::overlay_protocol::InfoSource {
                kind: crate::overlay_protocol::InfoKind::Description,
                path: String::new(),
                label: String::new(),
                text: "About".into(),
            }],
            shown: 0,
        }
    }

    #[test]
    fn a_turned_card_switches_sides_without_turning_back() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 2));
        keys.toggle("some", Side::Info, 0.0);
        assert!(keys.flip.is_none(), "no info yet");
        keys.receive_info(info("some"));
        keys.toggle("some", Side::Keys, 0.0);
        assert!(keys.face("some", FLIP_SECS).back);
        keys.take_requests();
        keys.toggle("some", Side::Info, FLIP_SECS);
        assert!(keys.showing_info("some") && !keys.showing("some"));
        assert!(keys.face("some", FLIP_SECS).back, "it stays turned");
        assert_eq!(
            keys.take_requests(),
            vec![FromOverlay::Info {
                mod_id: "some".into()
            }]
        );
        keys.press_selected(FLIP_SECS);
        assert!(keys.take_requests().is_empty(), "the info presses nothing");
        keys.toggle("some", Side::Info, FLIP_SECS);
        assert_eq!(keys.face("some", FLIP_SECS * 2.0), Face::FRONT);
    }

    #[test]
    fn the_info_scrolls_by_lines() {
        let mut keys = CardKeys::default();
        keys.receive_info(info("some"));
        keys.toggle("some", Side::Info, 0.0);
        keys.hidden_rows = 4;
        assert!(!keys.select(-1));
        assert!(keys.select(1));
        assert_eq!(keys.flip.as_ref().unwrap().scroll, info::STEP_LINES);
        assert!(keys.select(1));
        assert_eq!(keys.flip.as_ref().unwrap().scroll, 4);
        assert!(!keys.select(1), "below the last line is the categories row");
    }

    #[test]
    fn only_mods_with_hotkeys_turn_over() {
        let mut keys = CardKeys::default();
        keys.receive(answer("none", 0));
        keys.receive(answer("some", 2));
        assert!(!keys.has("none"));
        assert!(!keys.has("unknown"));
        assert!(keys.has("some"));
        keys.toggle("none", Side::Keys, 0.0);
        assert_eq!(keys.face("none", 1.0), Face::FRONT);
        keys.toggle("some", Side::Keys, 0.0);
        assert!(!keys.face("some", 0.0).back);
        let turned = keys.face("some", FLIP_SECS);
        assert!(turned.back && turned.settled());
    }

    #[test]
    fn turning_back_midway_starts_from_where_it_is() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 1));
        keys.toggle("some", Side::Keys, 0.0);
        let quarter = FLIP_SECS * 0.25;
        let before = keys.face("some", quarter);
        keys.toggle("some", Side::Keys, quarter);
        let after = keys.face("some", quarter);
        assert!(!before.back && !after.back);
        assert!((before.width - after.width).abs() < 1e-4);
        assert_eq!(keys.face("some", quarter * 2.0 + 1e-6), Face::FRONT);
        assert!(keys.flip.is_none());
    }

    #[test]
    fn the_selection_stays_on_the_rows_and_in_view() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 4));
        assert!(!keys.select(1), "nothing to select on the front");
        keys.toggle("some", Side::Keys, 0.0);
        keys.fit = 2;
        assert!(!keys.select(-1));
        assert!(keys.select(1));
        assert!(keys.select(1));
        let flip = keys.flip.as_ref().unwrap();
        assert_eq!((flip.selected, flip.scroll), (2, 1));
        assert!(keys.select(1));
        assert!(!keys.select(1), "below the last row is the categories row");
        for _ in 0..3 {
            assert!(keys.select(-1));
        }
        let flip = keys.flip.as_ref().unwrap();
        assert_eq!((flip.selected, flip.scroll), (0, 0));
        assert!(keys.select(1) && keys.select(1) && keys.select(1));
        keys.receive(answer("some", 2));
        assert_eq!(keys.flip.as_ref().unwrap().selected, 1);
    }

    #[test]
    fn space_asks_hestia_to_press_the_selected_key() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 3));
        keys.press_selected(0.0);
        assert!(keys.take_requests().is_empty(), "the front presses nothing");
        keys.toggle("some", Side::Keys, 0.0);
        keys.select(1);
        keys.press_selected(1.0);
        keys.press_selected(1.5);
        assert_eq!(
            keys.take_requests(),
            vec![
                FromOverlay::PressHotkey {
                    id: 1,
                    mod_id: "some".into(),
                    index: 1,
                    key: "VK_F1".into(),
                },
                FromOverlay::PressHotkey {
                    id: 2,
                    mod_id: "some".into(),
                    index: 1,
                    key: "VK_F1".into(),
                },
            ]
        );
    }

    #[test]
    fn an_answer_lights_the_key_or_gives_the_error() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 2));
        keys.toggle("some", Side::Keys, 0.0);
        keys.press_selected(1.0);
        keys.press_selected(1.1);
        assert_eq!(keys.presses.len(), 2);
        assert_eq!(keys.pressed(1, None, 2.0), None);
        let lit = keys.lit.as_ref().unwrap();
        assert_eq!((lit.mod_id.as_str(), lit.index, lit.at), ("some", 0, 2.0));
        assert_eq!(
            keys.pressed(2, Some("no".into()), 2.5),
            Some("no".to_owned())
        );
        assert_eq!(keys.lit.as_ref().unwrap().at, 2.0);
        assert!(keys.presses.is_empty());
        keys.press_selected(3.0);
        keys.press_selected(3.0 + ANSWER_SECS);
        assert_eq!(keys.presses.len(), 1);
    }

    #[test]
    fn a_greyed_out_key_is_not_pressed() {
        let mut keys = CardKeys::default();
        let mut hotkeys = answer("some", 2);
        hotkeys.hotkeys[1].unavailable = true;
        keys.receive(hotkeys);
        keys.toggle("some", Side::Keys, 0.0);
        keys.select(1);
        keys.press_selected(1.0);
        assert!(keys.take_requests().is_empty());
        assert!(keys.presses.is_empty());
        keys.select(-1);
        keys.press_selected(1.0);
        assert_eq!(keys.take_requests().len(), 1);
    }

    #[test]
    fn losing_the_focus_shows_the_front() {
        let mut keys = CardKeys::default();
        keys.receive(answer("some", 1));
        keys.toggle("some", Side::Keys, 0.0);
        keys.keep_focused(Some("some"));
        assert!(keys.face("some", FLIP_SECS).back);
        keys.keep_focused(Some("other"));
        assert_eq!(keys.face("some", FLIP_SECS), Face::FRONT);
    }
}

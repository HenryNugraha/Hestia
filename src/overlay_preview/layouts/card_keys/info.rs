//! The info side of a card's back: the mod's description, personal note or
//! one of its text files, read-only.  The header picks which, like the
//! source menu in the mod's details, and Space moves to the next.  The text
//! scrolls by lines, with W and S or the wheel over it.  Notes are written in
//! Hestia's window.

use std::collections::HashMap;

use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

use super::super::{
    ACCENT, chrome_alpha, content_color, content_gray, elided_galley, rgba_alpha,
    scale_color_alpha, text,
};
use super::{CardKeys, paint_scrollbar};
use crate::app::TextKey;
use crate::overlay_protocol::{InfoKind, InfoSource, ModInfo};

/// Lines one step of W, S or the wheel scrolls.
pub(in crate::overlay_preview::layouts) const STEP_LINES: usize = 3;
const MENU_ROW: f32 = 24.0;

#[derive(Default)]
pub(super) struct CardInfo {
    /// Mods' sources with text, by id, once Hestia has sent them.
    sources: HashMap<String, Vec<InfoSource>>,
    /// The source each mod shows.
    shown: HashMap<String, usize>,
    /// The source menu is open on the card showing its info.
    menu: bool,
}

impl CardInfo {
    /// Keeps the source the card showed when it's still there, else shows
    /// the one Hestia picked.
    pub(super) fn receive(&mut self, answer: ModInfo) {
        let picked = answer.sources.get(answer.shown).map(same_source);
        let before = self
            .sources
            .get(&answer.mod_id)
            .zip(self.shown.get(&answer.mod_id))
            .and_then(|(sources, &shown)| sources.get(shown))
            .map(same_source);
        let sources: Vec<InfoSource> = answer
            .sources
            .into_iter()
            .filter(|source| !source.text.trim().is_empty())
            .collect();
        let find = |wanted: &(InfoKind, String)| {
            sources
                .iter()
                .position(|source| same_source(source) == *wanted)
        };
        let shown = before
            .as_ref()
            .and_then(find)
            .or_else(|| picked.as_ref().and_then(find))
            .unwrap_or(0);
        self.shown.insert(answer.mod_id.clone(), shown);
        self.sources.insert(answer.mod_id, sources);
    }

    pub(super) fn has(&self, mod_id: &str) -> bool {
        self.sources
            .get(mod_id)
            .is_some_and(|sources| !sources.is_empty())
    }

    pub(super) fn close_menu(&mut self) {
        self.menu = false;
    }

    fn list(&self, mod_id: &str) -> &[InfoSource] {
        self.sources
            .get(mod_id)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn shown(&self, mod_id: &str) -> usize {
        self.shown.get(mod_id).copied().unwrap_or(0)
    }

    /// Shows the next source, after the last the first.  False when there's
    /// only one.
    pub(super) fn next_source(&mut self, mod_id: &str) -> bool {
        let count = self.list(mod_id).len();
        if count < 2 {
            return false;
        }
        let next = (self.shown(mod_id) + 1) % count;
        self.shown.insert(mod_id.to_owned(), next);
        true
    }
}

fn same_source(source: &InfoSource) -> (InfoKind, String) {
    (source.kind, source.path.clone())
}

fn source_icon(kind: InfoKind) -> lucide_icons::Icon {
    match kind {
        InfoKind::Description => lucide_icons::Icon::AlertCircle,
        InfoKind::Note => lucide_icons::Icon::NotebookPen,
        InfoKind::File => lucide_icons::Icon::FileText,
    }
}

fn source_label(source: &InfoSource) -> &str {
    match source.kind {
        InfoKind::Description => text(TextKey::LibraryMetaSourceDescription),
        InfoKind::Note => text(TextKey::LibraryPersonalNote),
        InfoKind::File if source.label.trim().is_empty() => &source.path,
        InfoKind::File => &source.label,
    }
}

fn icon_font() -> FontId {
    FontId::new(12.0, egui::FontFamily::Name("preview-icons".into()))
}

impl CardKeys {
    /// How many sources the mod's info has.
    pub(in crate::overlay_preview::layouts) fn info_sources(&self, mod_id: &str) -> usize {
        self.info.list(mod_id).len()
    }

    /// Shows the next source on the card showing its info.  False when
    /// there's none to move to.
    pub(in crate::overlay_preview::layouts) fn next_info_source(&mut self) -> bool {
        let Some(flip) = self
            .flip
            .as_mut()
            .filter(|flip| flip.back && flip.side == super::Side::Info)
        else {
            return false;
        };
        if !self.info.next_source(&flip.mod_id) {
            return false;
        }
        flip.scroll = 0;
        self.info.close_menu();
        true
    }

    /// The source in `header` and its text in `body`, from the scrolled-to
    /// line.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_info(
        &mut self,
        ui: &Ui,
        painter: &egui::Painter,
        id: egui::Id,
        header: Rect,
        body: Rect,
        mod_id: &str,
        settled: bool,
        reveal: f32,
        overlay_opacity: u8,
    ) {
        let fade = |color: Color32| scale_color_alpha(color, reveal);
        let sources = self.info.list(mod_id);
        let Some(source) = sources.get(self.info.shown(mod_id)) else {
            return;
        };
        let several = sources.len() > 1;

        // The header: what shows, and a menu of the others when there are.
        let label = elided_galley(
            ui,
            source_label(source),
            FontId::proportional(13.0),
            fade(content_gray(240, overlay_opacity)),
            header.width() - 18.0 - if several { 16.0 } else { 0.0 },
        );
        let chip = Rect::from_min_size(
            header.min - Vec2::new(4.0, 0.0),
            Vec2::new(
                (18.0 + label.size().x + if several { 16.0 } else { 0.0 } + 4.0)
                    .min(header.width() + 4.0),
                header.height(),
            ),
        );
        let mut toggle_menu = false;
        let hovered = settled && several && {
            let response = ui
                .interact(chip, id.with("info-source"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            toggle_menu = response.clicked();
            response.hovered()
        };
        if hovered || self.info.menu {
            painter.rect_filled(
                chip,
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
        let icon_color = fade(match source.kind {
            InfoKind::Note => content_color(ACCENT, overlay_opacity),
            _ => content_gray(200, overlay_opacity),
        });
        painter.text(
            egui::pos2(header.min.x + 7.0, header.center().y),
            Align2::CENTER_CENTER,
            char::from(source_icon(source.kind)).to_string(),
            icon_font(),
            icon_color,
        );
        let label_width = label.size().x;
        painter.galley(
            egui::pos2(
                header.min.x + 18.0,
                header.center().y - label.size().y * 0.5,
            ),
            label,
            Color32::PLACEHOLDER,
        );
        if several {
            painter.text(
                egui::pos2(header.min.x + 18.0 + label_width + 8.0, header.center().y),
                Align2::CENTER_CENTER,
                char::from(lucide_icons::Icon::ChevronDown).to_string(),
                icon_font(),
                fade(content_gray(180, overlay_opacity)),
            );
        }

        // The text, as many whole lines as fit.
        let mut job = egui::text::LayoutJob::simple(
            source.text.clone(),
            FontId::proportional(12.0),
            fade(content_gray(214, overlay_opacity)),
            body.width() - 8.0,
        );
        job.wrap.break_anywhere = false;
        let galley = ui.painter().layout_job(job);
        if toggle_menu {
            self.info.menu = !self.info.menu;
        }
        let line_height = galley
            .rows
            .first()
            .map_or(14.0, |row| row.rect().height())
            .max(1.0);
        let lines = galley.rows.len();
        let fit = ((body.height() / line_height).floor() as usize).max(1);
        let hidden = lines.saturating_sub(fit);
        if settled {
            self.hidden_rows = hidden;
            self.fit = fit;
        }
        let first = self.flip.as_mut().map_or(0, |flip| {
            flip.scroll = flip.scroll.min(hidden);
            flip.scroll
        });
        let top = galley.rows.get(first).map_or(0.0, |row| row.pos.y);
        let shown_height = fit.min(lines) as f32 * line_height;
        let clip = Rect::from_min_size(body.min, Vec2::new(body.width(), shown_height))
            .intersect(painter.clip_rect());
        painter.with_clip_rect(clip).galley(
            egui::pos2(body.min.x, body.min.y - top),
            galley,
            Color32::PLACEHOLDER,
        );
        if hidden > 0 {
            paint_scrollbar(
                painter,
                body,
                fit as f32 * line_height,
                fit,
                lines,
                first,
                reveal,
                overlay_opacity,
            );
        }
    }

    /// The open source menu under `header`, over everything else on the
    /// card.  A click off it closes it.
    pub(super) fn paint_info_menu(
        &mut self,
        ui: &Ui,
        id: egui::Id,
        card: Rect,
        header: Rect,
        mod_id: &str,
        overlay_opacity: u8,
    ) {
        if !self.info.menu {
            return;
        }
        let sources = self.info.list(mod_id);
        if sources.len() < 2 {
            self.info.menu = false;
            return;
        }
        let shown = self.info.shown(mod_id);
        let clicked_off = ui
            .interact(card, id.with("info-menu-off"), Sense::click())
            .clicked();
        let panel = Rect::from_min_size(
            egui::pos2(header.min.x - 4.0, header.max.y + 4.0),
            Vec2::new(
                card.width() - 2.0 * (header.min.x - card.min.x) + 8.0,
                sources.len() as f32 * MENU_ROW + 8.0,
            ),
        )
        .intersect(card.shrink(4.0));
        let painter = ui.painter().with_clip_rect(card);
        painter.rect_filled(
            panel,
            CornerRadius::same(5),
            Color32::from_black_alpha(chrome_alpha(240, overlay_opacity)),
        );
        painter.rect_stroke(
            panel,
            CornerRadius::same(5),
            Stroke::new(1.0, content_gray(68, overlay_opacity)),
            StrokeKind::Inside,
        );
        let mut picked = None;
        for (index, source) in sources.iter().enumerate() {
            let row = Rect::from_min_size(
                egui::pos2(
                    panel.min.x + 4.0,
                    panel.min.y + 4.0 + index as f32 * MENU_ROW,
                ),
                Vec2::new(panel.width() - 8.0, MENU_ROW),
            );
            if row.max.y > panel.max.y {
                break;
            }
            let response = ui
                .interact(row, id.with(("info-source", index)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                picked = Some(index);
            }
            if response.hovered() {
                painter.rect_filled(
                    row,
                    CornerRadius::same(4),
                    rgba_alpha(255, 255, 255, 26, overlay_opacity),
                );
            }
            let selected = index == shown;
            painter.text(
                egui::pos2(row.min.x + 10.0, row.center().y),
                Align2::CENTER_CENTER,
                char::from(source_icon(source.kind)).to_string(),
                icon_font(),
                match source.kind {
                    InfoKind::Note => content_color(ACCENT, overlay_opacity),
                    _ => content_gray(200, overlay_opacity),
                },
            );
            let label = elided_galley(
                ui,
                source_label(source),
                FontId::proportional(12.0),
                content_gray(if selected { 245 } else { 214 }, overlay_opacity),
                row.width() - 22.0 - 20.0,
            );
            painter.galley(
                egui::pos2(row.min.x + 22.0, row.center().y - label.size().y * 0.5),
                label,
                Color32::PLACEHOLDER,
            );
            if selected {
                painter.text(
                    egui::pos2(row.max.x - 10.0, row.center().y),
                    Align2::CENTER_CENTER,
                    char::from(lucide_icons::Icon::Check).to_string(),
                    icon_font(),
                    content_color(ACCENT, overlay_opacity),
                );
            }
        }
        if let Some(index) = picked {
            self.info.shown.insert(mod_id.to_owned(), index);
            self.info.menu = false;
            if let Some(flip) = &mut self.flip {
                flip.scroll = 0;
            }
        } else if clicked_off {
            self.info.menu = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(kind: InfoKind, path: &str, text: &str) -> InfoSource {
        InfoSource {
            kind,
            path: path.into(),
            label: String::new(),
            text: text.into(),
        }
    }

    fn answer(sources: Vec<InfoSource>, shown: usize) -> ModInfo {
        ModInfo {
            mod_id: "some".into(),
            sources,
            shown,
        }
    }

    #[test]
    fn empty_sources_are_left_out() {
        let mut info = CardInfo::default();
        info.receive(answer(vec![source(InfoKind::Description, "", "  ")], 0));
        assert!(!info.has("some"));
        info.receive(answer(
            vec![
                source(InfoKind::Description, "", ""),
                source(InfoKind::File, "readme.txt", "Hi"),
            ],
            1,
        ));
        assert!(info.has("some"));
        assert_eq!(info.list("some").len(), 1);
        assert_eq!(info.shown("some"), 0);
    }

    #[test]
    fn a_new_answer_keeps_the_source_that_showed() {
        let mut info = CardInfo::default();
        let sources = vec![
            source(InfoKind::Description, "", "About"),
            source(InfoKind::File, "readme.txt", "Hi"),
        ];
        info.receive(answer(sources.clone(), 0));
        assert!(info.next_source("some"));
        assert_eq!(info.shown("some"), 1);
        // A note written since comes before the files.
        let mut with_note = sources;
        with_note.insert(1, source(InfoKind::Note, "note.md", "Mine"));
        info.receive(answer(with_note, 0));
        assert_eq!(info.shown("some"), 2);
        assert!(info.next_source("some"));
        assert_eq!(info.shown("some"), 0);
    }

    #[test]
    fn one_source_has_no_next() {
        let mut info = CardInfo::default();
        info.receive(answer(vec![source(InfoKind::Description, "", "About")], 0));
        assert!(!info.next_source("some"));
    }
}

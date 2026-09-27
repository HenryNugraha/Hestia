//! Two-stage expansion within a fixed native canvas. Its bottom center never moves.

#[derive(Clone, Copy)]
struct Spring {
    value: f32,
    velocity: f32,
}

impl Spring {
    fn advance(&mut self, target: f32, omega: f32, dt: f32) {
        // Exact critically damped spring integration preserves velocity when
        // Alt is released or pressed again halfway through a transition.
        let offset = self.value - target;
        let rate = self.velocity + omega * offset;
        let decay = (-omega * dt).exp();
        self.value = (target + (offset + rate * dt) * decay).clamp(0.0, 1.0);
        self.velocity = (self.velocity - omega * rate * dt) * decay;
        if (self.value - target).abs() < 0.001 && self.velocity.abs() < 0.03 {
            self.value = target;
            self.velocity = 0.0;
        }
    }
}

pub(super) struct ExpansionMotion {
    strip: Spring,
    cards: Spring,
    previous_time: Option<f64>,
    expanded: bool,
}

impl ExpansionMotion {
    pub(super) fn new(expanded: bool) -> Self {
        let value = if expanded { 1.0 } else { 0.0 };
        Self {
            strip: Spring {
                value,
                velocity: 0.0,
            },
            cards: Spring {
                value,
                velocity: 0.0,
            },
            previous_time: None,
            expanded,
        }
    }

    pub(super) fn advance(&mut self, now: f64, expanded: bool) {
        let dt = self
            .previous_time
            .map_or(0.0, |last| (now - last).clamp(0.0, 0.1) as f32);
        self.previous_time = Some(now);
        self.expanded = expanded;
        // Open the strip first. On collapse, withdraw the cards before shrinking.
        let strip_target = if expanded || self.cards.value > 0.001 {
            1.0
        } else {
            0.0
        };
        self.strip.advance(strip_target, 32.0, dt);
        let cards_target = if expanded && self.strip.value >= 0.985 {
            1.0
        } else {
            0.0
        };
        self.cards.advance(cards_target, 26.0, dt);
    }

    pub(super) fn strip(&self) -> f32 {
        self.strip.value
    }
    pub(super) fn cards(&self) -> f32 {
        self.cards.value
    }

    pub(super) fn animating(&self) -> bool {
        let target = if self.expanded { 1.0 } else { 0.0 };
        self.strip.value != target || self.cards.value != target
    }
}

pub(super) struct StripGeometry {
    pub base: egui::Rect,
    pub header: egui::Rect,
    pub rail: egui::Rect,
    pub gallery: egui::Rect,
}

/// The strip at `progress` from idle to open.  A longer hotkey makes the idle
/// strip `idle_width` wide.
pub(super) fn geometry(bounds: egui::Rect, progress: f32, idle_width: f32) -> StripGeometry {
    let width = egui::lerp(idle_width..=super::EXPANDED_SIZE.x, progress).min(bounds.width());
    let height = egui::lerp(
        super::IDLE_SIZE.y..=(super::EXPANDED_SIZE.y - super::CAROUSEL_HEIGHT),
        progress,
    );
    let base = egui::Rect::from_min_size(
        egui::pos2(bounds.center().x - width / 2.0, bounds.bottom() - height),
        egui::vec2(width, height),
    );
    let header = egui::Rect::from_min_size(base.min, egui::vec2(width, super::HEADER_HEIGHT));
    let rail = egui::Rect::from_min_max(egui::pos2(base.left(), header.bottom()), base.max);
    let gallery = egui::Rect::from_min_size(
        egui::pos2(
            bounds.center().x - super::EXPANDED_SIZE.x / 2.0,
            base.top() - super::CAROUSEL_HEIGHT,
        ),
        egui::vec2(super::EXPANDED_SIZE.x, super::CAROUSEL_HEIGHT),
    );
    StripGeometry {
        base,
        header,
        rail,
        gallery,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bottom_center_is_invariant_through_expansion() {
        let bounds = egui::Rect::from_min_size(egui::pos2(-300.0, 20.0), egui::vec2(560.0, 660.0));
        for step in 0..=100 {
            let layout = geometry(bounds, step as f32 / 100.0, super::super::IDLE_SIZE.x);
            assert!(layout.base.center_bottom().distance(bounds.center_bottom()) < 0.0001);
        }
    }

    #[test]
    fn strip_opens_before_cards_and_closes_after_them() {
        let mut motion = ExpansionMotion::new(false);
        for frame in 0..120 {
            motion.advance(frame as f64 / 120.0, true);
            if motion.strip() < 0.985 {
                assert_eq!(motion.cards(), 0.0);
            }
        }
        assert!(!motion.animating());
        for frame in 120..300 {
            motion.advance(frame as f64 / 120.0, false);
            if motion.cards() > 0.001 {
                assert_eq!(motion.strip(), 1.0);
            }
        }
        assert_eq!(motion.strip(), 0.0);
        assert_eq!(motion.cards(), 0.0);
        assert!(!motion.animating());
    }

    #[test]
    fn reversing_mid_transition_preserves_position_and_velocity() {
        let mut motion = ExpansionMotion::new(false);
        motion.advance(0.0, true);
        motion.advance(0.07, true);
        let before = (motion.strip.value, motion.strip.velocity);
        motion.advance(0.07, false);
        assert_eq!((motion.strip.value, motion.strip.velocity), before);
        for frame in 1..=100 {
            motion.advance(0.07 + frame as f64 / 120.0, false);
        }
        assert_eq!(motion.strip(), 0.0);
    }
}

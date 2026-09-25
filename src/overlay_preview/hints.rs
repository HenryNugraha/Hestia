use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    Categories,
    Mods,
    Exclusive,
    Additive,
    Disable,
}

pub(super) const SEQUENCE: [Hint; 5] = [
    Hint::Categories,
    Hint::Mods,
    Hint::Exclusive,
    Hint::Additive,
    Hint::Disable,
];

const SPEED: f64 = 55.0;
const IDLE_REPAINT: Duration = Duration::from_secs(60);
const ANIMATED_REPAINT: Duration = Duration::from_millis(16);

#[derive(Default)]
pub(super) struct Ticker {
    opened_at: Option<f64>,
}

pub(super) struct Frame {
    pub offset: f32,
    pub repaint_after: Duration,
}

impl Ticker {
    pub fn update(&mut self, expanded: bool, now: f64) {
        if expanded {
            self.opened_at.get_or_insert(now);
        } else {
            self.opened_at = None;
        }
    }

    pub fn frame(&self, now: f64, animate: bool, cycle_width: f32) -> Frame {
        if !animate || !cycle_width.is_finite() || cycle_width <= 0.0 {
            return Frame {
                offset: 0.0,
                repaint_after: IDLE_REPAINT,
            };
        }

        let elapsed = (now - self.opened_at.unwrap_or(now)).max(0.0);
        let offset = ((elapsed * SPEED) % f64::from(cycle_width)) as f32;

        Frame {
            offset,
            repaint_after: ANIMATED_REPAINT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marquee_moves_linearly_and_wraps_without_a_jump() {
        let mut ticker = Ticker::default();
        ticker.update(true, 10.0);

        assert_eq!(ticker.frame(10.0, true, 100.0).offset, 0.0);
        assert_eq!(ticker.frame(11.0, true, 100.0).offset, 55.0);
        assert!((ticker.frame(11.1, true, 100.0).offset - 60.5).abs() < 0.001);
        assert!((ticker.frame(11.9, true, 100.0).offset - 4.5).abs() < 0.001);
        assert!((ticker.frame(11.9, true, 100.0).offset - 100.0).abs() > 0.1);
        assert_eq!(ticker.frame(12.0, true, 100.0).offset, 10.0);
    }

    #[test]
    fn navigation_does_not_restart_but_reopening_does() {
        let mut ticker = Ticker::default();
        ticker.update(true, 10.0);
        ticker.update(true, 15.0);
        assert_eq!(ticker.frame(15.0, true, 100.0).offset, 75.0);

        ticker.update(false, 16.0);
        ticker.update(true, 20.0);
        assert_eq!(ticker.frame(20.0, true, 100.0).offset, 0.0);
    }

    #[test]
    fn disabled_animation_is_static_and_does_not_request_frequent_repaints() {
        let mut ticker = Ticker::default();
        ticker.update(true, 10.0);

        let frame = ticker.frame(12.0, false, 100.0);
        assert_eq!(frame.offset, 0.0);
        assert_eq!(frame.repaint_after, IDLE_REPAINT);
    }

    #[test]
    fn invalid_cycle_width_is_static() {
        let mut ticker = Ticker::default();
        ticker.update(true, 10.0);

        for cycle_width in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let frame = ticker.frame(12.0, true, cycle_width);
            assert_eq!(frame.offset, 0.0);
            assert_eq!(frame.repaint_after, IDLE_REPAINT);
        }
    }
}

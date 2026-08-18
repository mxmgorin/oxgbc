//! Auto-repeat for a held direction, which SDL does for keys and not for pads: one
//! row per press is a long way down a shelf of a hundred carts.

use sdl2::controller::Button;
use std::time::{Duration, Instant};

/// How long a direction is held before it starts stepping, and how fast it steps
/// then. Between a keyboard's own rates and what a long list wants.
const DELAY: Duration = Duration::from_millis(350);
const INTERVAL: Duration = Duration::from_millis(110);

/// When the direction now held is due to step again. Holding a different one starts
/// the wait over: it is a new press, not the same one continuing.
#[derive(Default)]
pub struct Repeat {
    held: Option<Button>,
    due: Option<Instant>,
}

impl Repeat {
    /// `true` once per step, from `DELAY` after the press and every `INTERVAL` after
    /// that. The next step is counted from now rather than from when it was due, so a
    /// frame that ran late does not come back with a burst of them.
    pub fn stepped(&mut self, button: Button, now: Instant) -> bool {
        if self.held != Some(button) {
            self.held = Some(button);
            self.due = Some(now + DELAY);

            return false;
        }

        match self.due {
            Some(due) if now >= due => {
                self.due = Some(now + INTERVAL);

                true
            }
            _ => false,
        }
    }

    /// Nothing is held, or what is held must not repeat.
    pub fn clear(&mut self) {
        self.held = None;
        self.due = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_waits_out_the_delay_then_steps_on_the_interval() {
        let mut repeat = Repeat::default();
        let start = Instant::now();

        assert!(!repeat.stepped(Button::DPadDown, start), "the press itself");
        assert!(!repeat.stepped(Button::DPadDown, start + DELAY / 2));
        assert!(repeat.stepped(Button::DPadDown, start + DELAY));

        let stepped = start + DELAY;
        assert!(!repeat.stepped(Button::DPadDown, stepped + INTERVAL / 2));
        assert!(repeat.stepped(Button::DPadDown, stepped + INTERVAL));
    }

    #[test]
    fn another_direction_waits_out_the_delay_of_its_own() {
        let mut repeat = Repeat::default();
        let start = Instant::now();

        repeat.stepped(Button::DPadDown, start);
        assert!(repeat.stepped(Button::DPadDown, start + DELAY));

        let turned = start + DELAY;
        assert!(!repeat.stepped(Button::DPadRight, turned), "a new press");
        assert!(!repeat.stepped(Button::DPadRight, turned + INTERVAL));
        assert!(repeat.stepped(Button::DPadRight, turned + DELAY));
    }

    #[test]
    fn letting_go_takes_the_wait_with_it() {
        let mut repeat = Repeat::default();
        let start = Instant::now();

        repeat.stepped(Button::DPadUp, start);
        repeat.clear();

        assert!(
            !repeat.stepped(Button::DPadUp, start + DELAY),
            "pressed again"
        );
        assert!(repeat.stepped(Button::DPadUp, start + DELAY + DELAY));
    }
}

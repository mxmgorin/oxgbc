use crate::apu::channels::channel::ChannelType;
use crate::apu::registers::NRx3x4;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeriodTimer {
    counter: u16,
    ch_type: ChannelType,
    /// T-cycles left in the "countdown just reloaded" window after a step;
    /// period writes landing in it reseed the countdown immediately.
    #[serde(default)]
    just_reloaded: u8,
}

impl PeriodTimer {
    pub fn new(ch_type: ChannelType) -> Self {
        Self {
            counter: 0,
            ch_type,
            just_reloaded: 0,
        }
    }

    /// Ticks internal counter when enable conditions are met and returns true if expired
    pub fn tick(&mut self, nrx3x4: &NRx3x4) -> bool {
        if !self.is_expired() {
            self.counter -= 1;
        }

        if self.is_expired() {
            self.reload(nrx3x4);
            self.just_reloaded = 2;
            return true;
        }

        if self.just_reloaded > 0 {
            self.just_reloaded -= 1;
        }

        false
    }

    #[inline(always)]
    pub fn is_just_reloaded(&self) -> bool {
        self.just_reloaded > 0
    }

    /// Ticks (inclusive) until `tick` next returns true: an expired counter
    /// fires on the very next tick, otherwise after `counter` of them.
    #[inline(always)]
    pub fn fire_in(&self) -> usize {
        (self.counter as usize).max(1)
    }

    /// Batch equivalent of `n` fire-free ticks (`n < fire_in()`): the counter
    /// just decrements and the reload window drains.
    #[inline(always)]
    pub fn skip(&mut self, n: usize) {
        debug_assert!(n < self.fire_in());
        self.counter -= n as u16;
        self.just_reloaded = self.just_reloaded.saturating_sub(n.min(255) as u8);
    }

    #[inline(always)]
    pub fn counter(&self) -> u16 {
        self.counter
    }

    #[inline(always)]
    pub fn reload(&mut self, nrx3x4: &NRx3x4) {
        self.counter = (2048 - nrx3x4.get_period()) * self.get_multiplier();
    }

    /// Trigger reload with an explicit T-cycle latency instead of the steady
    /// `(2048 - period) * mult` cadence (square-channel trigger delays).
    #[inline(always)]
    pub fn reload_with_delay(&mut self, nrx3x4: &NRx3x4, delay: u16) {
        self.counter = (2047 - nrx3x4.get_period()) * self.get_multiplier() + delay;
    }

    #[inline(always)]
    pub fn is_expired(&self) -> bool {
        self.counter == 0
    }

    #[inline(always)]
    fn get_multiplier(&self) -> u16 {
        match self.ch_type {
            ChannelType::CH1 | ChannelType::CH2 => 4,
            ChannelType::CH4 => unreachable!("CH4 doesn't have period timer"),
            ChannelType::CH3 => 2,
        }
    }
}

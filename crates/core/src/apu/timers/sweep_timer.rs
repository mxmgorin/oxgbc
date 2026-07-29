use crate::apu::channels::channel::ChannelType;
use crate::apu::channels::square_channel::NR10;
use crate::apu::registers::NRx3x4;
use crate::apu::NR52;
use serde::{Deserialize, Serialize};

/// SameBoy's pipelined sweep (CGB D/E): the 128 Hz frame event applies the
/// previously computed addend to the period and schedules a recalculation +
/// overflow check `shift` 1 MHz cycles later (behind a short reload delay).
/// NR10 writes and retriggers interact with the in-flight calculation.
#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SweepTimer {
    pub nr10: NR10,
    /// 3-bit up-counter clocked at 128 Hz; the sweep fires when it hits 7.
    #[serde(default)]
    countdown: u8,
    /// 1 MHz cycles until the scheduled recalculation completes.
    #[serde(default)]
    calculate_countdown: u8,
    /// 1 MHz cycles before `calculate_countdown` starts running.
    #[serde(default)]
    reload_timer: u8,
    /// A zero-shift calculation completed instantly at schedule time; the
    /// overflow check still runs when the reload delay expires.
    #[serde(default)]
    instant_calculation_done: bool,
    /// The last scheduled calculation had shift 0 (it still counts down).
    #[serde(default)]
    unshifted: bool,
    /// Period snapshot used by the two-step add (refreshed when a
    /// calculation completes outside the restart hold).
    #[serde(default)]
    shadow_sample_length: u16,
    /// The delta applied at the next 128 Hz fire.
    #[serde(default)]
    addend: u16,
    /// Addend of the last completed calculation (NR10 writes overflow-check
    /// against it).
    #[serde(default)]
    completed_addend: u16,
    /// Trigger-to-sweep-start hold, in 2 MHz cycles.
    #[serde(default)]
    restart_hold: u8,
}

impl SweepTimer {
    /// Work in flight on the 1/2 MHz grids: a scheduled recalc, its reload
    /// delay, the restart hold, or an instant calc awaiting its overflow
    /// check. The batch APU advance falls back to per-tick while this holds.
    #[inline(always)]
    pub fn is_live(&self) -> bool {
        self.calculate_countdown != 0
            || self.reload_timer != 0
            || self.restart_hold != 0
            || self.instant_calculation_done
    }

    /// 1 MHz pipeline tick.
    pub fn tick_1mhz(&mut self, nr52: &mut NR52, nrx3x4: &NRx3x4) {
        if self.reload_timer > 0 {
            self.reload_timer -= 1;

            if self.reload_timer == 0 {
                if self.calculate_countdown == 0 && self.instant_calculation_done {
                    self.calculation_done(nr52, nrx3x4);
                }

                self.instant_calculation_done = false;
            }

            return;
        }

        // The calculation pauses while the shift bits are 0, unless it was
        // scheduled as a zero-shift one.
        if self.calculate_countdown > 0 && (self.nr10.individual_step() != 0 || self.unshifted) {
            self.calculate_countdown -= 1;

            if self.calculate_countdown == 0 {
                self.calculation_done(nr52, nrx3x4);
            }
        }
    }

    /// 2 MHz tick for the restart hold window.
    #[inline(always)]
    pub fn tick_2mhz(&mut self) {
        if self.restart_hold > 0 {
            self.restart_hold -= 1;
        }
    }

    /// APU bug: the period is checked after adding the sweep delta twice.
    fn calculation_done(&mut self, nr52: &mut NR52, nrx3x4: &NRx3x4) {
        if self.restart_hold == 0 {
            self.shadow_sample_length = nrx3x4.get_period();
        }

        if self.nr10.direction_down() {
            self.addend ^= 0x7FF;
        }

        if self.shadow_sample_length + self.addend > 0x7FF && !self.nr10.direction_down() {
            #[cfg(feature = "apu-trace")]
            eprintln!(
                "sweep calc_done kill: shadow={:03X} addend={:03X} hold={}",
                self.shadow_sample_length, self.addend, self.restart_hold
            );
            nr52.deactivate_ch(ChannelType::CH1);
        }

        self.completed_addend = self.addend;
    }

    /// 128 Hz frame-sequencer event.
    pub fn on_frame_event(&mut self, nrx3x4: &mut NRx3x4, lf_odd: bool) {
        self.countdown = self.countdown.wrapping_add(1) & 7;
        self.trigger_calculation(nrx3x4, lf_odd);
    }

    /// Fires the sweep when due: applies the pending addend to the period
    /// and schedules the next recalculation + overflow check.
    fn trigger_calculation(&mut self, nrx3x4: &mut NRx3x4, lf_odd: bool) {
        if self.nr10.pace() != 0 && self.countdown == 7 {
            let shift = self.nr10.individual_step();

            if shift != 0 {
                let period = (self.addend
                    + self.shadow_sample_length
                    + self.nr10.direction_down() as u16)
                    & 0x7FF;
                nrx3x4.set_period(period);
            }

            if self.restart_hold == 0 {
                self.addend = nrx3x4.get_period() >> shift;
            }

            // Recalculation and overflow check only occur after a delay.
            self.calculate_countdown = shift;
            self.reload_timer = 1 + lf_odd as u8;
            self.unshifted = shift == 0;
            self.countdown = ((self.nr10.byte >> 4) & 7) ^ 7;

            if self.calculate_countdown == 0 {
                self.instant_calculation_done = true;
            }
        }
    }

    /// NR10 write: glitches the in-flight calculation, overflow-checks the
    /// completed addend against the new negate bit, and may fire the sweep.
    pub fn on_nr10_write(
        &mut self,
        value: u8,
        nr52: &mut NR52,
        nrx3x4: &mut NRx3x4,
        lf_odd: bool,
    ) {
        if self.calculate_countdown != 0 || self.reload_timer != 0 {
            // Countdown just reloaded: re-reload it from the new shift.
            if self.reload_timer == 2 {
                self.calculate_countdown = value & 7;

                if self.calculate_countdown == 0 {
                    self.reload_timer = 0;
                }
            }

            if (value & 7) != 0
                && (self.nr10.byte & 7) == 0
                && !lf_odd
                && self.calculate_countdown > 1
            {
                self.calculate_countdown -= 1;

                if self.calculate_countdown == 0 {
                    self.calculation_done(nr52, nrx3x4);
                }
            }
        }

        let old_negate = self.nr10.direction_down();
        self.nr10.byte = value;

        if self.shadow_sample_length + self.completed_addend + old_negate as u16 > 0x7FF
            && !self.nr10.direction_down()
        {
            #[cfg(feature = "apu-trace")]
            eprintln!(
                "sweep nr10 kill: shadow={:03X} completed={:03X} old_neg={}",
                self.shadow_sample_length, self.completed_addend, old_negate
            );
            nr52.deactivate_ch(ChannelType::CH1);
        }

        self.trigger_calculation(nrx3x4, lf_odd);
    }

    /// NR14 trigger (channel 1 only). APU bug: a nonzero shift schedules an
    /// overflow check right at the trigger.
    pub fn on_trigger(&mut self, was_active: bool, nrx3x4: &NRx3x4, lf_odd: bool) {
        self.instant_calculation_done = false;
        self.shadow_sample_length = 0;
        self.completed_addend = 0;
        let shift = self.nr10.individual_step();

        if shift != 0 {
            self.calculate_countdown = shift;
            self.reload_timer = 2 + !was_active as u8;
            self.unshifted = false;
            self.addend = nrx3x4.get_period() >> shift;
        } else {
            self.addend = 0;
        }

        // CGB-E: 2 - lf + 2 (calibrated +2 for our write-to-tick offset).
        self.restart_hold = 6 - lf_odd as u8;
        self.countdown = ((self.nr10.byte >> 4) & 7) ^ 7;
    }
}

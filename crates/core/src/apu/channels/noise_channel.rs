use crate::apu::channels::channel::ChannelType;
use crate::apu::dac::{DacEnable, DigitalSampleProducer};
use crate::apu::registers::NRx1;
use crate::apu::registers::{NRx2, NRx4};
use crate::apu::timers::envelope_timer::EnvelopeTimer;
use crate::apu::timers::length_timer::LengthTimer;
use crate::apu::NR52;
use crate::get_bit_flag;
use serde::{Deserialize, Serialize};

pub const CH4_START_ADDRESS: u16 = NR41_CH4_LENGTH_TIMER_ADDRESS;
pub const CH4_END_ADDRESS: u16 = NR44_CH4_CONTROL_ADDRESS;

pub const NR41_CH4_LENGTH_TIMER_ADDRESS: u16 = 0xFF20;
pub const NR42_CH4_VOLUME_ENVELOPE_ADDRESS: u16 = 0xFF21;
pub const NR43_CH4_FREQUENCY_RANDOMNESS_ADDRESS: u16 = 0xFF22;
pub const NR44_CH4_CONTROL_ADDRESS: u16 = 0xFF23;
pub const NR44_CH4_UNUSED_MASK: u8 = 0b0011_1111;

/// Noise runs a free 14-bit counter; the LFSR steps on the rising edge of the
/// counter bit selected by the NR43 shift (SameBoy's model). The counter
/// increments every `divisor * 8` T-cycles (4 when the divisor code is 0), so
/// an LFSR step period is `divisor * 16 << shift` — the classic table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseChannel {
    nrx1_len: NRx1,
    nrx2_envelope_and_dac: NRx2,
    nr43_freq_and_rnd: NR43,
    nrx4_ctrl: NRx4,

    length_timer: LengthTimer,
    envelope_timer: EnvelopeTimer,
    /// 15-bit LFSR, XNOR feedback, all-zero start (SameBoy convention:
    /// the DAC input is bit 0 as-is).
    lfsr: u16,
    /// Free-running 14-bit counter whose bit edges clock the LFSR.
    #[serde(default)]
    counter: u16,
    /// T-cycles until the next counter increment.
    #[serde(default)]
    counter_countdown: u16,
    /// The countdown was reloaded on the previous T-cycle (an NR43 write in
    /// that window re-seeds the countdown with an alignment-dependent value).
    #[serde(default)]
    countdown_reloaded: bool,
    /// The counter stepped at least once since the last trigger.
    #[serde(default)]
    did_step_counter: bool,
    /// The channel was triggered with the DAC on; cleared on APU off and DAC
    /// disable. Keeps the counter running.
    #[serde(default)]
    counter_active: bool,
    /// The counter keeps counting in the background after any trigger, even
    /// with the channel inactive; cleared on APU off.
    #[serde(default)]
    background_active: bool,
    /// Last trigger happened with the DAC disabled.
    #[serde(default)]
    started_with_dac_disabled: bool,
    /// Output latched at the last LFSR step.
    #[serde(default)]
    current_sample: u8,
}

impl Default for NoiseChannel {
    fn default() -> NoiseChannel {
        let ch_type = ChannelType::CH4;

        Self {
            nrx1_len: NRx1::new(ch_type),
            nrx2_envelope_and_dac: Default::default(),
            nr43_freq_and_rnd: Default::default(),
            nrx4_ctrl: Default::default(),
            length_timer: LengthTimer::new(ch_type),
            envelope_timer: Default::default(),
            lfsr: 0,
            counter: 0,
            counter_countdown: 0,
            countdown_reloaded: false,
            did_step_counter: false,
            counter_active: false,
            background_active: false,
            started_with_dac_disabled: false,
            current_sample: 0,
        }
    }
}

impl DacEnable for NoiseChannel {
    fn is_dac_enabled(&self) -> bool {
        self.nrx2_envelope_and_dac.is_dac_enabled()
    }
}

impl DigitalSampleProducer for NoiseChannel {
    fn get_sample(&self, master_ctrl: NR52) -> u8 {
        if master_ctrl.is_ch4_on() {
            return self.current_sample * self.envelope_timer.get_volume();
        }

        0
    }
}

impl NoiseChannel {
    #[inline]
    pub fn read(&self, addr: u16) -> u8 {
        // Write-only bits read back as 1; NR42/NR43 are fully readable.
        match addr {
            NR41_CH4_LENGTH_TIMER_ADDRESS => 0xFF,
            NR42_CH4_VOLUME_ENVELOPE_ADDRESS => self.nrx2_envelope_and_dac.byte,
            NR43_CH4_FREQUENCY_RANDOMNESS_ADDRESS => self.nr43_freq_and_rnd.byte,
            NR44_CH4_CONTROL_ADDRESS => self.nrx4_ctrl.read() | NR44_CH4_UNUSED_MASK | 0x80,
            _ => panic!("Invalid NoiseChannel address: {addr:#X}"),
        }
    }

    #[inline]
    pub fn write(
        &mut self,
        addr: u16,
        value: u8,
        master_ctrl: &mut NR52,
        len_first_half: bool,
        alignment: u8,
    ) {
        match addr {
            NR41_CH4_LENGTH_TIMER_ADDRESS => {
                self.nrx1_len.byte = value;
                self.length_timer.reload(self.nrx1_len);
            }
            NR42_CH4_VOLUME_ENVELOPE_ADDRESS => {
                let old = self.nrx2_envelope_and_dac.byte;
                let active = master_ctrl.is_ch4_on();
                self.nrx2_envelope_and_dac.byte = value;
                let dac_enabled = self.nrx2_envelope_and_dac.is_dac_enabled();

                if !dac_enabled {
                    // Disabling the DAC stops the background counter with a
                    // final adjustment when it was about to step.
                    if active && self.nr43_freq_and_rnd.clock_divider() != 0 {
                        if self.counter_countdown <= 4 {
                            self.counter = self.counter.wrapping_add(1) & 0x3FFF;
                        }

                        self.background_active = false;
                    }

                    self.counter_active = false;
                } else if active {
                    self.envelope_timer.nrx2_glitch(value, old);
                }

                master_ctrl.on_dac_update(dac_enabled, ChannelType::CH4);
            }
            NR43_CH4_FREQUENCY_RANDOMNESS_ADDRESS => {
                // A write that lands on the very cycle the counter countdown
                // reloaded re-seeds it with an alignment-dependent value.
                if self.countdown_reloaded {
                    let divisor = (((value & 7) as u16) << 2).max(2);
                    let adjust = if divisor == 2 {
                        0
                    } else {
                        [2u16, 1, 0, 3][(alignment & 3) as usize]
                    };
                    self.counter_countdown = (divisor + adjust) * 2;
                }

                self.nr43_freq_and_rnd.byte = value;
            }
            NR44_CH4_CONTROL_ADDRESS => {
                let was_len_enabled = self.nrx4_ctrl.is_length_enabled();
                self.nrx4_ctrl.write(value);

                if len_first_half && !was_len_enabled && self.nrx4_ctrl.is_length_enabled() {
                    self.length_timer
                        .extra_clock(master_ctrl, self.nrx4_ctrl.is_triggered());
                }

                if self.nrx4_ctrl.is_triggered() {
                    self.trigger(master_ctrl, len_first_half, alignment);
                }
            }
            _ => panic!("Invalid NoiseChannel address: {:#X}", addr),
        }
    }

    /// Ticks every T-cycle regardless of the NR52 status: once triggered, the
    /// counter keeps counting in the background, but the LFSR only steps
    /// while the channel is active.
    #[inline]
    /// Returns whether the LFSR stepped — the only point where this tick can
    /// change the channel's digital output.
    pub fn tick(&mut self, active: bool) -> bool {
        if !(self.counter_active || self.background_active) {
            return false;
        }

        if self.counter_countdown == 0 {
            self.counter_countdown = self.divisor_ticks();
        }

        self.counter_countdown -= 1;
        self.countdown_reloaded = false;

        if self.counter_countdown == 0 {
            self.counter_countdown = self.divisor_ticks();
            self.countdown_reloaded = true;

            let mask = 1u16 << self.nr43_freq_and_rnd.clock_shift();
            let old_bit = self.counter & mask != 0;
            self.counter = self.counter.wrapping_add(1) & 0x3FFF;
            self.did_step_counter = true;
            let new_bit = self.counter & mask != 0;

            if new_bit && !old_bit && active {
                self.step_lfsr();
                return true;
            }
        }

        false
    }

    /// Ticks (inclusive) until the counter next steps (`usize::MAX` while
    /// fully inactive — the countdown is frozen then).
    #[inline(always)]
    pub fn fire_in(&self) -> usize {
        if !(self.counter_active || self.background_active) {
            return usize::MAX;
        }

        if self.counter_countdown == 0 {
            self.divisor_ticks() as usize
        } else {
            self.counter_countdown as usize
        }
    }

    /// Batch equivalent of `n` step-free ticks (`1 <= n < fire_in()`): the
    /// lazily-reloaded countdown just drains.
    #[inline(always)]
    pub fn skip(&mut self, n: usize) {
        if !(self.counter_active || self.background_active) {
            return;
        }

        if self.counter_countdown == 0 {
            self.counter_countdown = self.divisor_ticks();
        }

        self.counter_countdown -= n as u16;
        self.countdown_reloaded = false;
    }

    /// T-cycles between counter increments: `divisor * 8`, or 4 for the 0
    /// divisor code.
    #[inline(always)]
    fn divisor_ticks(&self) -> u16 {
        let d = ((self.nr43_freq_and_rnd.clock_divider() as u16) << 2).max(2);
        d * 2
    }

    #[inline]
    fn step_lfsr(&mut self) {
        // XNOR of the two low bits shifts in from the top; in 7-bit (narrow)
        // mode bit 6 is written as well, and the explicit clear matters when
        // switching widths mid-run.
        let high_mask: u16 = if self.nr43_freq_and_rnd.lfsr_width() {
            0x4040
        } else {
            0x4000
        };
        let new_high = (self.lfsr ^ (self.lfsr >> 1) ^ 1) & 1 != 0;
        self.lfsr >>= 1;

        if new_high {
            self.lfsr |= high_mask;
        } else {
            self.lfsr &= !high_mask;
        }

        self.current_sample = (self.lfsr & 1) as u8;
    }

    #[inline]
    pub fn tick_length(&mut self, master_ctrl: &mut NR52) {
        self.length_timer.tick(master_ctrl, &mut self.nrx4_ctrl);
    }

    #[inline]
    pub fn tick_envelope(&mut self) {
        self.envelope_timer.tick(self.nrx2_envelope_and_dac);
    }

    #[inline]
    pub fn countdown_envelope(&mut self) {
        self.envelope_timer.countdown_tick();
    }

    #[inline]
    pub fn arm_envelope(&mut self, master_ctrl: &NR52) {
        self.envelope_timer
            .arm(self.nrx2_envelope_and_dac, master_ctrl.is_ch4_on());
    }

    /// APU power-off: the counter state and LFSR reset fully.
    #[inline]
    pub fn power_off(&mut self) {
        self.lfsr = 0;
        self.counter = 0;
        self.counter_countdown = 0;
        self.countdown_reloaded = false;
        self.did_step_counter = false;
        self.counter_active = false;
        self.background_active = false;
        self.started_with_dac_disabled = false;
        self.current_sample = 0;
    }

    /// SameBoy's prepare_noise_start for CGB D/E: the fresh countdown gets a
    /// pile of alignment/divisor-dependent adjustments (in T-cycles = 2x the
    /// reference 2 MHz units).
    fn trigger(&mut self, nr52: &mut NR52, len_first_half: bool, alignment: u8) {
        // `active` state before this trigger takes effect.
        let active = nr52.is_ch4_on();

        if self.length_timer.is_expired() {
            let extra = len_first_half && self.nrx4_ctrl.is_length_enabled();
            self.length_timer.reset(extra);
        }

        self.envelope_timer.reload(self.nrx2_envelope_and_dac);

        self.lfsr = 0;
        self.current_sample = 0;

        let dac_on = self.nrx2_envelope_and_dac.is_dac_enabled();
        let was_started_with_dac_disabled = self.started_with_dac_disabled;
        self.counter_active = dac_on;
        self.started_with_dac_disabled = !dac_on;
        let was_background = self.background_active;
        self.background_active = true;

        let mut divisor = (self.nr43_freq_and_rnd.byte & 7) as i32;
        let shift_bits = self.nr43_freq_and_rnd.byte & 0xF0;
        let mut instant_step = false;
        let mut div_1_glitch = false;

        if divisor > 1 && self.counter_countdown == 2 {
            self.counter = self.counter.wrapping_add(1) & 0x3FFF;
        } else if self.counter_countdown == 4 && alignment & 3 == 0 && active {
            if divisor == 0 {
                divisor = 8;
            } else if divisor == 1 {
                if !self.did_step_counter {
                    div_1_glitch = true;
                }

                let mask = 1u16 << self.nr43_freq_and_rnd.clock_shift();
                let old_bit = self.counter & mask != 0;
                self.counter = self.counter.wrapping_add(1) & 0x3FFF;

                if self.counter & mask != 0 && !old_bit {
                    instant_step = true;
                }
            }
        }

        let mut cd: i32 = if divisor == 0 { 6 } else { divisor * 4 + 6 };

        if alignment & 1 == 1 {
            if divisor == 0 {
                cd += if was_background { -1 } else { 1 };
            } else if alignment & 2 != 0 {
                if divisor == 1 && !active {
                    cd += 1;
                } else {
                    cd -= 3;
                }
            } else {
                cd -= 1;

                if divisor == 1 && active {
                    cd -= 4;
                }
            }
        } else if divisor != 0 {
            if alignment & 2 != 0 {
                cd -= 2;
            } else if divisor > 1 {
                cd -= 4;
            } else if divisor == 1 && active && shift_bits == 0 {
                cd -= 4;
            }
        }

        // Background counting glitches.
        if divisor > 1 {
            if !self.counter_active && alignment & 3 == 0 {
                cd += 4;
            }
        } else if was_background && !active && alignment & 3 == 0 {
            if divisor == 0 {
                if was_started_with_dac_disabled {
                    cd += 28;
                }
            } else {
                cd -= 4;
            }
        }

        if div_1_glitch {
            cd -= 4;
        }

        self.counter_countdown = (cd * 2).max(1) as u16;
        self.did_step_counter = alignment & 3 == 2;

        // Arbitrary but hardware-confirmed LFSR seed for this edge case.
        if divisor == 0 && active && alignment & 3 == 3 {
            self.lfsr = 0x0055;
        }

        if instant_step {
            self.step_lfsr();
        }

        if dac_on {
            nr52.activate_ch4();
        }
    }
}

/// FF22 — NR43: Channel 4 frequency & randomness
/// This register allows controlling the way the amplitude is randomly switched.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NR43 {
    byte: u8,
}

impl NR43 {
    #[inline]
    pub fn clock_shift(&self) -> u8 {
        self.byte >> 4
    }

    #[inline]
    pub fn get_lfsr_width(&self) -> LfsrWidth {
        if self.lfsr_width() {
            LfsrWidth::Bit7
        } else {
            LfsrWidth::Bit15
        }
    }

    #[inline]
    pub fn lfsr_width(&self) -> bool {
        get_bit_flag(self.byte, 3)
    }

    #[inline]
    pub fn clock_divider(&self) -> u8 {
        self.byte & 0b0000_0111
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum LfsrWidth {
    Bit15 = 0,
    Bit7 = 1,
}

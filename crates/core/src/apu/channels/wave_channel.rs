use crate::apu::channels::channel::ChannelType;
use crate::apu::dac::{DacEnable, DigitalSampleProducer};
use crate::apu::registers::{NRx1, NRx3x4};
use crate::apu::timers::length_timer::LengthTimer;
use crate::apu::timers::period_timer::PeriodTimer;
use crate::apu::NR52;
use serde::{Deserialize, Serialize};

pub const CH3_START_ADDRESS: u16 = CH3_NR30_DAC_ENABLE_ADDRESS;
pub const CH3_END_ADDRESS: u16 = CH3_NR33_PERIOD_HIGH_CONTROL_ADDRESS;

pub const CH3_NR30_DAC_ENABLE_ADDRESS: u16 = 0xFF1A;
pub const CH3_NR30_UNUSED_MASK: u8 = 0b0111_1111;

pub const CH3_NR31_LENGTH_TIMER_ADDRESS: u16 = 0xFF1B;
pub const CH3_NR32_OUTPUT_LEVEL_ADDRESS: u16 = 0xFF1C;
pub const CH3_NR33_PERIOD_LOW_ADDRESS: u16 = 0xFF1D;
pub const CH3_NR33_PERIOD_HIGH_CONTROL_ADDRESS: u16 = 0xFF1E;

pub const CH3_WAVE_RAM_START: u16 = 0xFF30;
pub const CH3_WAVE_RAM_END: u16 = 0xFF3F;
pub const CH3_NR30_DAC_ENABLE_POS: u8 = 7;

impl DacEnable for WaveChannel {
    fn is_dac_enabled(&self) -> bool {
        self.nrx0_dac_enable.is_dac_enabled()
    }
}

impl DigitalSampleProducer for WaveChannel {
    fn get_sample(&self, nr52: NR52) -> u8 {
        if nr52.is_ch3_on() {
            return self.wave_ram.current_nibble() >> self.volume_shift;
        }

        0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaveChannel {
    // registers
    nrx0_dac_enable: NR30,
    nrx1_length_timer: NRx1,
    nrx2_output_level: NR32,
    nrx3x4_period_and_ctrl: NRx3x4,
    pub wave_ram: WaveRam,

    // other data
    // todo: Period changes (written to NR33 or NR34) only take effect after the following time wave RAM is read
    period_timer: PeriodTimer,
    length_timer: LengthTimer,
    volume_shift: u8,
}

impl Default for WaveChannel {
    fn default() -> Self {
        Self {
            nrx0_dac_enable: Default::default(),
            nrx1_length_timer: NRx1::new(ChannelType::CH3),
            nrx2_output_level: Default::default(),
            nrx3x4_period_and_ctrl: Default::default(),
            wave_ram: Default::default(),
            length_timer: LengthTimer::new(ChannelType::CH3),
            period_timer: PeriodTimer::new(ChannelType::CH3),
            volume_shift: 0,
        }
    }
}

impl WaveChannel {
    #[inline]
    pub fn read(&self, address: u16) -> u8 {
        // Write-only bits read back as 1.
        match address {
            CH3_NR30_DAC_ENABLE_ADDRESS => self.nrx0_dac_enable.read(),
            CH3_NR31_LENGTH_TIMER_ADDRESS => 0xFF, // write-only
            CH3_NR32_OUTPUT_LEVEL_ADDRESS => self.nrx2_output_level.read() | 0x9F,
            CH3_NR33_PERIOD_LOW_ADDRESS => 0xFF, // write-only
            CH3_NR33_PERIOD_HIGH_CONTROL_ADDRESS => {
                self.nrx3x4_period_and_ctrl.nrx4.read() | 0xBF
            }
            _ => panic!("Invalid WaveChannel address: {address:#X}"),
        }
    }

    #[inline]
    pub fn write(&mut self, address: u16, value: u8, master_ctrl: &mut NR52, len_first_half: bool) {
        match address {
            CH3_NR30_DAC_ENABLE_ADDRESS => {
                self.nrx0_dac_enable.byte = value;
                master_ctrl.on_dac_update(self.nrx0_dac_enable.is_dac_enabled(), ChannelType::CH3);
            }
            CH3_NR31_LENGTH_TIMER_ADDRESS => {
                self.nrx1_length_timer.byte = value;
                self.length_timer.reload(self.nrx1_length_timer); // research: do it must be reloaded after write?
            }
            CH3_NR32_OUTPUT_LEVEL_ADDRESS => {
                self.nrx2_output_level.byte = value;
                // The output level change applies to the already-latched
                // sample immediately, not at the next trigger.
                self.volume_shift = self.nrx2_output_level.get_volume_shift();
            }
            CH3_NR33_PERIOD_LOW_ADDRESS => self.nrx3x4_period_and_ctrl.period_low.write(value),
            CH3_NR33_PERIOD_HIGH_CONTROL_ADDRESS => {
                let was_len_enabled = self.nrx3x4_period_and_ctrl.nrx4.is_length_enabled();
                self.nrx3x4_period_and_ctrl.nrx4.write(value);
                let nrx4 = self.nrx3x4_period_and_ctrl.nrx4;

                if len_first_half && !was_len_enabled && nrx4.is_length_enabled() {
                    self.length_timer
                        .extra_clock(master_ctrl, nrx4.is_triggered());
                }

                if nrx4.is_triggered() {
                    self.trigger(master_ctrl, len_first_half);
                }
            }
            _ => panic!("Invalid WaveChannel address: {:#X}", address),
        }
    }

    #[inline]
    pub fn tick_length(&mut self, master_ctrl: &mut NR52) {
        self.length_timer
            .tick(master_ctrl, &mut self.nrx3x4_period_and_ctrl.nrx4);
    }

    /// Returns whether the sample position advanced — the only point where
    /// this tick can change the channel's digital output.
    #[inline]
    pub fn tick(&mut self) -> bool {
        if self.period_timer.tick(&self.nrx3x4_period_and_ctrl) {
            self.wave_ram.inc_sample_index();
            return true;
        }

        false
    }

    /// Ticks (inclusive) until the next wave step.
    #[inline(always)]
    pub fn fire_in(&self) -> usize {
        self.period_timer.fire_in()
    }

    /// Batch equivalent of `n` step-free ticks (`n < fire_in()`).
    #[inline(always)]
    pub fn skip(&mut self, n: usize) {
        self.period_timer.skip(n);
    }

    #[inline]
    fn trigger(&mut self, master_ctrl: &mut NR52, len_first_half: bool) {
        let was_active = master_ctrl.is_ch3_on();

        if self.length_timer.is_expired() {
            let extra = len_first_half && self.nrx3x4_period_and_ctrl.nrx4.is_length_enabled();
            self.length_timer.reset(extra);
        }

        // Retriggering exactly when the sample countdown expired latches
        // WAV[0] immediately; otherwise the stale byte keeps playing until
        // the first step.
        self.wave_ram.reset_sample_index();

        if was_active && self.period_timer.is_expired() {
            self.wave_ram.reload_sample_buffer();
        }

        // The first step after a trigger comes 2 extra 2 MHz cycles late
        // (SameBoy: sample_countdown = (length ^ 0x7FF) + 3; calibrated
        // against same-suite channel_3_delay).
        self.period_timer
            .reload_with_delay(&self.nrx3x4_period_and_ctrl, 8);
        self.volume_shift = self.nrx2_output_level.get_volume_shift();

        if self.nrx0_dac_enable.is_dac_enabled() {
            master_ctrl.activate_ch3();
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WaveRam {
    // 32 samples, 4 bit each
    bytes: [u8; 16],
    sample_index: usize,
    /// Whole wave RAM byte latched at the last sample step; the output
    /// nibble is selected by the index parity.
    sample_buffer: u8,
}

impl WaveRam {
    /// CPU access while the channel plays (CGB): the bus hits the byte the
    /// channel is currently reading instead of the addressed one.
    #[inline]
    pub fn read(&self, addr: u16, ch_active: bool) -> u8 {
        if ch_active {
            return self.bytes[self.sample_index / 2];
        }

        let addr = addr - CH3_WAVE_RAM_START;
        self.bytes[addr as usize]
    }

    #[inline]
    pub fn write(&mut self, addr: u16, value: u8, ch_active: bool) {
        if ch_active {
            self.bytes[self.sample_index / 2] = value;
            return;
        }

        let index = addr - CH3_WAVE_RAM_START;
        self.bytes[index as usize] = value;
    }

    /// Output nibble of the latched byte: even sample index = high nibble.
    #[inline]
    fn current_nibble(&self) -> u8 {
        if self.sample_index % 2 == 0 {
            self.sample_buffer >> 4
        } else {
            self.sample_buffer & 0x0F
        }
    }

    #[inline]
    pub fn inc_sample_index(&mut self) {
        self.sample_index = (self.sample_index + 1) % 32;
        self.sample_buffer = self.bytes[self.sample_index / 2];
    }

    /// Trigger: the position restarts but the latched byte is NOT reloaded
    /// yet ("we don't change the sample just yet", SameBoy) — the first
    /// output after a trigger is the high nibble of the stale byte.
    #[inline]
    pub fn reset_sample_index(&mut self) {
        self.sample_index = 0;
    }

    #[inline]
    pub fn reload_sample_buffer(&mut self) {
        self.sample_buffer = self.bytes[self.sample_index / 2];
    }

    #[inline]
    pub fn clear_sample_buffer(&mut self) {
        self.sample_index = 0;
        self.sample_buffer = 0;
    }
}

// DAC enable
#[derive(Clone, Debug, Default, Copy, Serialize, Deserialize)]
pub struct NR30 {
    byte: u8,
}

impl NR30 {
    pub fn is_dac_enabled(&self) -> bool {
        (self.byte >> CH3_NR30_DAC_ENABLE_POS) != 0
    }

    pub fn read(&self) -> u8 {
        // Only the DAC-enable bit is readable; the rest read as 1.
        self.byte | CH3_NR30_UNUSED_MASK
    }
}

/// Output level
#[derive(Clone, Debug, Default, Copy, Serialize, Deserialize)]
pub struct NR32 {
    byte: u8,
}

impl NR32 {
    pub fn read(&self) -> u8 {
        self.byte | 0b1001_1111
    }

    pub fn get_volume_shift(&self) -> u8 {
        match self.byte & 0b0110_0000 {
            0b0000_0000 => 4, // mute
            0b0010_0000 => 0, // 100%
            0b0100_0000 => 1, // 50%
            0b0110_0000 => 2, // 25%
            _ => unreachable!(),
        }
    }
}

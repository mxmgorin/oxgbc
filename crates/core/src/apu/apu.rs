use crate::apu::channels::channel::ChannelType;
use crate::apu::channels::noise_channel::NR41_CH4_LENGTH_TIMER_ADDRESS;
use crate::apu::channels::noise_channel::{NoiseChannel, CH4_END_ADDRESS, CH4_START_ADDRESS};
use crate::apu::channels::square_channel::{
    SquareChannel, CH1_END_ADDRESS, CH1_START_ADDRESS, CH2_END_ADDRESS, CH2_START_ADDRESS,
};
use crate::apu::channels::square_channel::{
    NR11_CH1_LEN_TIMER_DUTY_CYCLE_ADDRESS, NR21_CH2_LEN_TIMER_DUTY_CYCLE_ADDRESS,
};
use crate::apu::channels::wave_channel::{
    WaveChannel, CH3_END_ADDRESS, CH3_START_ADDRESS, CH3_WAVE_RAM_END, CH3_WAVE_RAM_START,
};
use crate::apu::dac::{apply_dac, DacEnable, DigitalSampleProducer};
use crate::apu::hpf::Hpf;
use crate::apu::mixer::Mixer;
use crate::cpu::CPU_CLOCK_SPEED;
use crate::{change_f32_rounded, get_bit_flag, set_bit};
use serde::{Deserialize, Serialize};

pub const AUDIO_START_ADDRESS: u16 = 0xFF10;
pub const AUDIO_END_ADDRESS: u16 = 0xFF26;

pub const APU_CLOCK_SPEED: u16 = 512;
pub const AUDIO_MASTER_CONTROL_ADDRESS: u16 = 0xFF26;
pub const SOUND_PLANNING_ADDRESS: u16 = 0xFF25;
pub const MASTER_VOLUME_ADDRESS: u16 = 0xFF24;

pub const FRAME_SEQUENCER_DIV: u16 = (CPU_CLOCK_SPEED / APU_CLOCK_SPEED as u32) as u16;
/// Raw 16-bit DIV bit whose falling edge clocks the 512 Hz frame sequencer:
/// one edge per `FRAME_SEQUENCER_DIV` counts (two bit periods), hence `- 1`.
/// Double speed uses the next bit up. This is visible DIV bit 4 (DIV =
/// raw >> 8), matching `sequence_frame`.
pub const DIV_APU_BIT: u16 = FRAME_SEQUENCER_DIV.trailing_zeros() as u16 - 1;
pub const SAMPLING_FREQUENCY: u32 = 44_100;
/// Dynamic rate control may nudge the emission rate this far from nominal
/// (~0.5%) — enough to absorb clock drift, too small to hear as a pitch shift.
pub const MAX_SAMPLE_RATE_SKEW: u32 = SAMPLING_FREQUENCY / 200;

fn default_sample_rate() -> u32 {
    SAMPLING_FREQUENCY
}

/// Unreachable as a real key (bits 40+ are never set), so it forces the mix
/// recompute on the first tick after construction or an old savestate.
fn invalid_mix_key() -> u64 {
    u64::MAX
}

fn default_true() -> bool {
    true
}

/// Rational tanh approximation: monotonic, unit slope at 0, saturating to
/// ±1 toward |x| = 3. The mixer output times the volume cap (2.0) stays well
/// inside the valid range.
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApuConfig {
    pub buffer_size: usize,
    pub volume: f32,
    /// User-facing mute mask, bit N audible = channel N+1 in the mix. Not a
    /// hardware feature — NR51 panning and channel status are unaffected.
    #[serde(default = "default_channel_mask")]
    pub channel_mask: u8,
}

pub fn default_channel_mask() -> u8 {
    0x0F
}

impl ApuConfig {
    pub fn new(buffer_size: usize, volume: f32) -> Self {
        Self {
            buffer_size,
            volume,
            channel_mask: default_channel_mask(),
        }
    }

    pub fn change_volume(&mut self, delta: f32) {
        let val = change_f32_rounded(self.volume, delta);
        self.volume = val.clamp(0.0, 2.0);
    }
}

impl Default for ApuConfig {
    fn default() -> Self {
        Self::new(512, 1.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Apu {
    // internal
    ch1: SquareChannel,
    ch2: SquareChannel,
    ch3: WaveChannel,
    ch4: NoiseChannel,
    nr52: NR52,
    mixer: Mixer,

    // other data
    /// Frame sequencer phase, incremented per DIV-APU event (SameBoy's
    /// div_divider): length clocks when it becomes odd, sweep when
    /// `& 3 == 3`, envelope when `& 7 == 0`. Its parity while idle is the
    /// length-period half that the NRx4 extra-clocking quirks key on.
    frame_sequencer_step: u8,
    /// Last sampled DIV-APU bit for falling-edge detection (the frame
    /// sequencer is clocked by DIV, not by an internal timer).
    #[serde(default)]
    prev_div_apu_bit: bool,
    /// Powering the APU on while the DIV-APU bit is set makes the first
    /// falling edge a leftover of the pre-power period: it is swallowed, and
    /// the one after it runs without advancing the phase
    /// (same-suite div_write_trigger_10).
    #[serde(default)]
    skip_div_event: SkipDivEvent,
    /// 2 MHz cycle counter for the 1 MHz phase (SameBoy's lf_div), reseeded
    /// at APU power-on — the phase is deterministic from NR52, not from
    /// absolute time (same-suite channel_2_align vs channel_2_duty).
    #[serde(default)]
    lf_ticks: u32,
    ticks_count: u32,
    /// Ticks ahead provably free of channel fires and sample emissions, so a
    /// window skips in O(1). DIV-APU edges excluded (re-checked per window).
    /// 0 = recompute. Derived, rebuilt lazily, out of the savestate.
    #[serde(skip)]
    event_horizon: u32,
    /// Fractional phase of the output-sample clock: gains `sample_rate` per
    /// 4 MHz tick, emits a sample and wraps at `CPU_CLOCK_SPEED`. Emission is
    /// thus exactly `sample_rate` Hz of emulated time — no truncation drift
    /// against the audio device (4194304/95 would be ~44145 Hz, +0.1%).
    #[serde(default)]
    sample_acc: u32,
    /// Output sample rate in Hz, nominally [`SAMPLING_FREQUENCY`]; the
    /// frontend's dynamic rate control nudges it within
    /// [`MAX_SAMPLE_RATE_SKEW`] to keep its audio queue at the target fill.
    #[serde(default = "default_sample_rate")]
    sample_rate: u32,
    /// Running sums of the mixed analog output over the current inter-sample
    /// window; divided by `window_ticks` at emission (box-filter average).
    #[serde(default)]
    window_left: f32,
    #[serde(default)]
    window_right: f32,
    #[serde(default)]
    window_ticks: u32,
    /// Digital state the analog mix is a pure function of, packed by
    /// [`Self::pack_mix_key`] for change detection. It holds for hundreds of
    /// ticks between duty/wave/LFSR steps, so the float mix reruns only when
    /// it changes; a tick otherwise just extends the run of the cached value.
    #[serde(default = "invalid_mix_key")]
    mix_key: u64,
    /// Analog mix cached for the current `mix_key`.
    #[serde(default)]
    cached_left: f32,
    #[serde(default)]
    cached_right: f32,
    /// Ticks spent at the cached mix since it was last folded into the
    /// window sums.
    #[serde(default)]
    run_ticks: u32,
    /// Forces the next tick to re-derive the mix key: set by every APU
    /// register write, since write quirks can change a channel's digital
    /// output without a waveform step. Defaults to true so fresh state and
    /// old savestates recompute immediately.
    #[serde(default = "default_true")]
    mix_dirty: bool,
    /// Mix inputs watched by direct compare instead of a dirty flag, packed:
    /// NR52 (sweep overflow deactivates ch1 outside `write`, off the DIV-APU
    /// grid) and the user channel mask (the frontend pokes the config
    /// directly). NR51/NR50 need no watch: they change only through `write`.
    #[serde(default)]
    mix_cold: u32,
    buffer_idx: usize,
    buffer: Box<[f32]>,
    hpf: Hpf,
    pub config: ApuConfig,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new(ApuConfig::default())
    }
}

impl Apu {
    pub fn new(mut config: ApuConfig) -> Self {
        if config.buffer_size % 2 != 0 {
            config.buffer_size += 1; // we need even buffer
        }

        Self {
            ch1: SquareChannel::new_ch1(),
            ch2: SquareChannel::new_ch2(),
            ch3: WaveChannel::default(),
            ch4: NoiseChannel::default(),
            nr52: NR52::default(),
            mixer: Default::default(),
            frame_sequencer_step: 0,
            prev_div_apu_bit: false,
            skip_div_event: SkipDivEvent::None,
            lf_ticks: 0,
            ticks_count: 0,
            event_horizon: 0,
            sample_acc: 0,
            sample_rate: SAMPLING_FREQUENCY,
            window_left: 0.0,
            window_right: 0.0,
            window_ticks: 0,
            mix_key: invalid_mix_key(),
            cached_left: 0.0,
            cached_right: 0.0,
            run_ticks: 0,
            mix_dirty: true,
            mix_cold: 0,
            buffer: vec![0.0; config.buffer_size].into_boxed_slice(),
            buffer_idx: 0,
            hpf: Hpf::new(SAMPLING_FREQUENCY),
            config,
        }
    }

    #[inline(always)]
    pub fn update_buffer_size(&mut self) {
        self.buffer = vec![0.0; self.config.buffer_size].into_boxed_slice();
        self.clear_buffer();
    }

    /// Applies the resolved hardware model (the HPF leak differs per model).
    pub fn set_model(&mut self, model: crate::emu::config::GbModel) {
        self.hpf.set_model(SAMPLING_FREQUENCY, model);
    }

    /// Sets the output sample rate, clamped to nominal ± [`MAX_SAMPLE_RATE_SKEW`].
    /// The clamp doubles as a guard against degenerate frontend math (a NaN
    /// cast to u32 is 0, which would silence the APU forever).
    #[inline(always)]
    pub fn set_sample_rate(&mut self, rate: u32) {
        self.event_horizon = 0; // the emission horizon depends on the rate
        self.sample_rate = rate.clamp(
            SAMPLING_FREQUENCY - MAX_SAMPLE_RATE_SKEW,
            SAMPLING_FREQUENCY + MAX_SAMPLE_RATE_SKEW,
        );
    }

    /// Batch equivalent of one `tick(bit)` per device tick over a window.
    /// `v_first` is the DIV value the first tick sees, `step` its per-tick
    /// increment (2 in double speed), `shift` selects the DIV-APU bit.
    /// Spans between fires, DIV-APU edges and emissions collapse to counter
    /// arithmetic; anything stateful runs through the real `tick()` at its tick.
    pub fn advance(&mut self, device_ticks: usize, v_first: u16, step: u16, shift: u16) {
        // The trace counts individual ticks; keep it exact.
        #[cfg(feature = "apu-trace")]
        {
            for j in 0..device_ticks {
                let v = v_first.wrapping_add(step.wrapping_mul(j as u16));
                self.tick(v >> shift & 1 != 0);
            }
        }

        #[cfg(not(feature = "apu-trace"))]
        {
            // Fast path: whole window with nothing due. The horizon is cached;
            // the DIV-APU edge is not — `v_first` is fresh each window, so the
            // shift check below also catches DIV writes moving the bit phase.
            let bit_first = v_first >> shift & 1 != 0;
            let cold = self.nr52.byte as u32 | (self.config.channel_mask as u32) << 8;

            if self.event_horizon as usize >= device_ticks
                && !self.mix_dirty
                && cold == self.mix_cold
                && self.prev_div_apu_bit == bit_first
                && !self.ch1.sweep_pipeline_live()
            {
                let boundary = 1u32 << shift;
                let into = v_first as u32 & (boundary - 1);
                // edge-free iff the last tick's value stays below the boundary
                let edge_free =
                    into + (device_ticks as u32 - 1) * step as u32 + 1 <= boundary;

                if edge_free {
                    self.skip_ticks(device_ticks);
                    self.event_horizon -= device_ticks as u32;
                    return;
                }
            }

            self.advance_slow(device_ticks, v_first, step, shift);
        }
    }

    /// Uncommon window: something due, dirty or in flight. Real `tick()`s at
    /// events, O(1) skips between, leaves a fresh horizon for the fast path.
    #[cfg(not(feature = "apu-trace"))]
    fn advance_slow(&mut self, device_ticks: usize, v_first: u16, step: u16, shift: u16) {
        let mut remaining = device_ticks;
        // counter value observed by the immediately-next tick
        let mut v_next = v_first;
        let boundary = 1u32 << shift;

        while remaining > 0 {
            let bit_next = v_next >> shift & 1 != 0;
            let cold = self.nr52.byte as u32 | (self.config.channel_mask as u32) << 8;

            // stateful-in-flight → execute the real tick
            if self.mix_dirty
                || cold != self.mix_cold
                || self.prev_div_apu_bit != bit_next
                || self.ch1.sweep_pipeline_live()
            {
                self.tick(bit_next);
                v_next = v_next.wrapping_add(step);
                remaining -= 1;
                continue;
            }

            // ticks until the next possible event (inclusive)
            let to_edge = {
                let into = v_next as u32 & (boundary - 1);
                ((boundary - into) as usize).div_ceil(step as usize) + 1
            };
            let span = remaining.min(to_edge).min(self.fire_or_emit_in());

            if span <= 1 {
                self.tick(bit_next);
                v_next = v_next.wrapping_add(step);
                remaining -= 1;
                continue;
            }

            // consume span-1 provably event-free ticks in O(1)
            let n = span - 1;
            self.skip_ticks(n);
            v_next = v_next.wrapping_add(step.wrapping_mul(n as u16));
            remaining -= n;
        }

        // hand the fast path a fresh horizon (edges are its own problem)
        self.event_horizon = if self.mix_dirty || self.ch1.sweep_pipeline_live() {
            0
        } else {
            (self.fire_or_emit_in() - 1) as u32
        };
    }

    /// Ticks (inclusive) until the earliest channel fire or sample emission.
    #[cfg(not(feature = "apu-trace"))]
    #[inline(always)]
    fn fire_or_emit_in(&self) -> usize {
        let mut k = (CPU_CLOCK_SPEED - self.sample_acc).div_ceil(self.sample_rate) as usize;
        if self.nr52.is_ch1_on() {
            k = k.min(self.ch1.fire_in());
        }
        if self.nr52.is_ch2_on() {
            k = k.min(self.ch2.fire_in());
        }
        if self.nr52.is_ch3_on() {
            k = k.min(self.ch3.fire_in());
        }
        k.min(self.ch4.fire_in())
    }

    /// Batch equivalent of `n` provably event-free device ticks.
    #[cfg(not(feature = "apu-trace"))]
    #[inline(always)]
    fn skip_ticks(&mut self, n: usize) {
        self.lf_ticks = self.lf_ticks.wrapping_add(n as u32);
        if self.nr52.is_ch1_on() {
            self.ch1.skip(n);
        }
        if self.nr52.is_ch2_on() {
            self.ch2.skip(n);
        }
        if self.nr52.is_ch3_on() {
            self.ch3.skip(n);
        }
        self.ch4.skip(n);
        self.run_ticks += n as u32;
        self.sample_acc += n as u32 * self.sample_rate;
    }

    #[inline(always)]
    pub fn tick(&mut self, div_apu_bit: bool) {
        // Only the trace output reads this counter; the field stays for the
        // savestate layout (postcard is positional).
        #[cfg(feature = "apu-trace")]
        {
            self.ticks_count = self.ticks_count.wrapping_add(1);
        }
        self.lf_ticks = self.lf_ticks.wrapping_add(1);
        // Any DIV-APU edge can move a digital output: falling edges clock
        // length/envelope/sweep, rising edges arm envelopes.
        let div_edge = self.prev_div_apu_bit != div_apu_bit;
        self.sequence_frame(div_apu_bit);

        // The sweep pipeline runs regardless of the channel state: the
        // scheduled recalculation on the 1 MHz grid, the restart hold on the
        // 2 MHz grid.
        let tick_1mhz = self.lf_ticks & 3 == 3;
        let tick_2mhz = self.lf_ticks & 1 == 1;
        self.ch1
            .tick_sweep_pipeline(&mut self.nr52, tick_1mhz, tick_2mhz);

        // Inactive channels do not clock their frequency timers: the duty /
        // sample position stays frozen until the next trigger.
        let mut stepped = false;
        if self.nr52.is_ch1_on() {
            stepped |= self.ch1.tick();
        }
        if self.nr52.is_ch2_on() {
            #[cfg(feature = "apu-trace")]
            let before = self.ch2.duty_sequence;
            stepped |= self.ch2.tick();
            #[cfg(feature = "apu-trace")]
            if before != self.ch2.duty_sequence {
                eprintln!(
                    "CH2 step -> {} t={}",
                    self.ch2.duty_sequence, self.ticks_count
                );
            }
        }
        if self.nr52.is_ch3_on() {
            stepped |= self.ch3.tick();
        }
        // The noise counter free-runs in the background once triggered; only
        // the LFSR stepping is gated on the channel being active.
        stepped |= self.ch4.tick(self.nr52.is_ch4_on());

        // Integrate the analog mix over the whole inter-sample window (a box
        // filter) instead of point-sampling it once per 95 ticks: transitions
        // land in the average with sub-sample weight, attenuating the
        // square-wave harmonics above Nyquist that used to alias back down as
        // inharmonic ringing. Running the DAC+mix float path on all 4 M ticks
        // is too slow for real games, so two layers gate it: a cheap check of
        // the events that can move a digital input (a waveform step, a
        // DIV-APU edge, a register write, one of the directly-watched bytes),
        // then a compare of the packed digital state itself. Between changes
        // a tick costs a few integer ops and a run-length increment.
        let cold = self.nr52.byte as u32 | (self.config.channel_mask as u32) << 8;

        if stepped || div_edge || self.mix_dirty || cold != self.mix_cold {
            self.mix_dirty = false;
            self.mix_cold = cold;

            let key = self.pack_mix_key();
            if key != self.mix_key {
                self.flush_mix_run();
                self.mix_key = key;
                (self.hpf.dac1_enabled, self.mixer.sample1) = apply_dac(self.nr52, &self.ch1);
                (self.hpf.dac2_enabled, self.mixer.sample2) = apply_dac(self.nr52, &self.ch2);
                (self.hpf.dac3_enabled, self.mixer.sample3) = apply_dac(self.nr52, &self.ch3);
                (self.hpf.dac4_enabled, self.mixer.sample4) = apply_dac(self.nr52, &self.ch4);
                (self.cached_left, self.cached_right) = self.mixer.mix(self.config.channel_mask);
            }
        }
        self.run_ticks += 1;

        self.sample_acc += self.sample_rate;
        if self.sample_acc >= CPU_CLOCK_SPEED {
            self.sample_acc -= CPU_CLOCK_SPEED;
            self.flush_mix_run();

            let scale = 1.0 / self.window_ticks as f32;
            let (output_left, output_right) = self
                .hpf
                .apply_filter(self.window_left * scale, self.window_right * scale);
            self.window_left = 0.0;
            self.window_right = 0.0;
            self.window_ticks = 0;

            self.push_buffer(output_left, output_right);
        }
    }

    /// Digital state the mixer output is a function of, packed for change
    /// detection: channel samples in bits 0-15, DAC enables in 16-19, NR51 in
    /// 20-27, NR50 in 28-35, the user channel mask in 36-39. The cached mix
    /// stays correct against any event the tick gate lets through spuriously
    /// (a duty step to the same sample, a DIV edge that clocked nothing) —
    /// the key, not the gate, decides whether the float path reruns.
    #[inline]
    fn pack_mix_key(&self) -> u64 {
        let dacs = self.ch1.is_dac_enabled() as u64
            | (self.ch2.is_dac_enabled() as u64) << 1
            | (self.ch3.is_dac_enabled() as u64) << 2
            | (self.ch4.is_dac_enabled() as u64) << 3;

        self.ch1.get_sample(self.nr52) as u64
            | (self.ch2.get_sample(self.nr52) as u64) << 4
            | (self.ch3.get_sample(self.nr52) as u64) << 8
            | (self.ch4.get_sample(self.nr52) as u64) << 12
            | dacs << 16
            | (self.mixer.nr51_panning.byte as u64) << 20
            | (self.mixer.nr50_volume.byte as u64) << 28
            | (self.config.channel_mask as u64) << 36
    }

    /// Folds the run of ticks spent at the cached mix into the window sums.
    #[inline(always)]
    fn flush_mix_run(&mut self) {
        if self.run_ticks > 0 {
            let run = self.run_ticks as f32;
            self.window_left += self.cached_left * run;
            self.window_right += self.cached_right * run;
            self.window_ticks += self.run_ticks;
            self.run_ticks = 0;
        }
    }

    #[inline(always)]
    pub fn push_buffer(&mut self, output_left: f32, output_right: f32) {
        let mut output_left = output_left * self.config.volume;
        let mut output_right = output_right * self.config.volume;

        // Volume above 1.0 can push the signal past the backend's [-1, 1]
        // range, which hard-clips with harsh harmonics; saturate smoothly
        // instead. Gated so the default volume stays bit-exact linear.
        if self.config.volume > 1.0 {
            output_left = soft_clip(output_left);
            output_right = soft_clip(output_right);
        }

        let buffer_len = self.buffer.len();
        debug_assert!(buffer_len % 2 == 0);

        // SAFETY:
        // - `buffer` is aligned to `f32`, which guarantees at least 4-byte alignment.
        // - we create the buffer using `into_boxed_slice` (or `Vec<f32>`), the allocator
        //   ensures the alignment is at least 8 bytes, which is sufficient for `u64`.
        // - `buffer.len()` is guaranteed to be even (checked at creation), so dividing by 2
        //   to reinterpret as `u64` slices is safe and valid.
        let buffer_u64 = unsafe {
            std::slice::from_raw_parts_mut(self.buffer.as_mut_ptr() as *mut u64, buffer_len / 2)
        };

        // SAFETY:
        // - `[f32; 2]` and `u64` have the same size (8 bytes).
        // - `transmute` here performs a bitwise reinterpretation of the two floats as a single u64,
        //   preserving their exact bit pattern without changing the data.
        // - All bit patterns are valid for both `f32` and `u64`, so this is safe and defined behavior.
        // - Endianness affects byte order but does not affect safety or correctness for raw packing.
        let packed = unsafe { std::mem::transmute::<[f32; 2], u64>([output_left, output_right]) };

        if self.buffer_idx >= buffer_len {
            self.clear_buffer();
        }

        // SAFETY:
        // - `self.buffer_idx` is always even and within bounds of `buffer_u64`.
        // - this is the only place where `buffer_idx` is written to.
        unsafe {
            *buffer_u64.get_unchecked_mut(self.buffer_idx / 2) = packed;
        }

        self.buffer_idx += 2;
    }

    #[inline(always)]
    pub fn get_buffer(&self) -> &[f32] {
        &self.buffer[0..self.buffer_idx]
    }

    #[inline(always)]
    pub fn clear_buffer(&mut self) {
        self.buffer_idx = 0;
    }

    #[inline(always)]
    pub fn buffer_ready(&self) -> bool {
        self.buffer_idx >= self.config.buffer_size / 2
    }

    #[inline(always)]
    pub fn write(&mut self, address: u16, value: u8, double_speed: bool) {
        self.mix_dirty = true;

        if (CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END).contains(&address) {
            self.ch3
                .wave_ram
                .write(address, value, self.nr52.is_ch3_on());
            return;
        }

        if !self.nr52.is_audio_on() && address != AUDIO_MASTER_CONTROL_ADDRESS {
            //return; //todo: research Asteroids tries to write to panning before enabling
        }

        // the length timers (in NRx1) on monochrome models also writable event when turned off
        let value = if !self.nr52.is_audio_on()
            && [
                NR11_CH1_LEN_TIMER_DUTY_CYCLE_ADDRESS,
                NR21_CH2_LEN_TIMER_DUTY_CYCLE_ADDRESS,
                NR41_CH4_LENGTH_TIMER_ADDRESS,
            ]
            .contains(&address)
        {
            value & 0b0011_1111
        } else {
            value
        };

        // First half of a length period: the sequencer phase parity says the
        // next event does NOT clock length. NRx4 writes in this phase get the
        // extra length clocking quirks.
        let len_first_half = self.frame_sequencer_step & 1 == 1;
        // 1 MHz phase of the 2 MHz APU clock (SameBoy's lf_div): trigger
        // delays depend on it. Reseeded at APU power-on, see `lf_ticks`.
        let lf_odd = self.lf_ticks & 2 != 0;

        #[cfg(feature = "apu-trace")]
        if address == 0xFF19 || address == 0xFF18 {
            eprintln!(
                "NR2x write {address:04X}={value:02X} t={} lf={}",
                self.ticks_count, lf_odd
            );
        }

        // Trigger-to-first-duty-step latency for an inactive square channel;
        // an active one restarts 4 T-cycles sooner (see SquareChannel). In
        // double speed a CPU write lands on the opposite half of the 2 MHz
        // period, flipping the sign of the phase term (calibrated against
        // same-suite channel_2 delay/align/duty).
        let trigger_delay = if double_speed {
            12 + 2 * lf_odd as u16
        } else {
            12 - 2 * lf_odd as u16
        };

        match address {
            CH1_START_ADDRESS..=CH1_END_ADDRESS => self.ch1.write(
                address,
                value,
                &mut self.nr52,
                len_first_half,
                trigger_delay,
                lf_odd,
            ),
            CH2_START_ADDRESS..=CH2_END_ADDRESS => self.ch2.write(
                address,
                value,
                &mut self.nr52,
                len_first_half,
                trigger_delay,
                lf_odd,
            ),
            CH3_START_ADDRESS..=CH3_END_ADDRESS => {
                self.ch3
                    .write(address, value, &mut self.nr52, len_first_half)
            }
            CH4_START_ADDRESS..=CH4_END_ADDRESS => {
                // 2 MHz cycle alignment (SameBoy's noise_channel.alignment).
                let alignment = ((self.lf_ticks >> 1) & 3) as u8;
                self.ch4
                    .write(address, value, &mut self.nr52, len_first_half, alignment)
            }
            AUDIO_MASTER_CONTROL_ADDRESS => {
                let prev_enable = self.nr52.is_audio_on();
                self.nr52.write(value);

                if !prev_enable && self.nr52.is_audio_on() {
                    // turning on
                    self.ch3.wave_ram.clear_sample_buffer();
                    // The 1 MHz phase restarts at power-on (SameBoy seeds
                    // lf_div = 1).
                    self.lf_ticks = 0;

                    // Power-on while the DIV-APU bit is set: the leftover
                    // falling edge is swallowed and the sequencer starts in
                    // the first half of a length period (phase 1).
                    if self.prev_div_apu_bit {
                        self.skip_div_event = SkipDivEvent::Skip;
                        self.frame_sequencer_step = 1;
                    }
                } else if prev_enable && !self.nr52.is_audio_on() {
                    // turning_off
                    for addr in CH1_START_ADDRESS..=0xFF25 {
                        self.write(addr, 0x00, double_speed);
                    }

                    self.frame_sequencer_step = 0;
                    self.skip_div_event = SkipDivEvent::None;
                    self.ch1.reset_duty();
                    self.ch2.reset_duty();
                    self.ch3.wave_ram.reset_sample_index();
                    self.ch4.power_off();
                }
            }
            SOUND_PLANNING_ADDRESS => self.mixer.nr51_panning.byte = value,
            MASTER_VOLUME_ADDRESS => self.mixer.nr50_volume.byte = value,
            CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END => {
                self.ch3
                    .wave_ram
                    .write(address, value, self.nr52.is_ch3_on())
            }
            _ => {
                if (AUDIO_START_ADDRESS..=AUDIO_END_ADDRESS).contains(&address) {
                    return;
                }

                panic!("Invalid APU address: {address:x}");
            }
        }
    }

    /// Register state the boot ROM leaves behind: master power on with the
    /// ch1 status flag set, boot-beep duty/envelope in ch1, default panning
    /// and volume (mooneye boot_hwio).
    pub fn set_boot_state(&mut self) {
        self.mix_dirty = true;
        self.nr52.byte = 0x81;
        self.mixer.nr50_volume.byte = 0x77;
        self.mixer.nr51_panning.byte = 0xF3;
        self.write(0xFF11, 0x80, false); // NR11: duty 2 (the boot beep), length 0
        self.write(0xFF12, 0xF3, false); // NR12: initial volume 15, decreasing, pace 3
    }

    /// PCM12 ($FF76, CGB): current digital output of channels 1 (low nibble)
    /// and 2 (high nibble).
    #[inline(always)]
    pub fn read_pcm12(&self) -> u8 {
        let v = self.ch1.get_sample(self.nr52) | (self.ch2.get_sample(self.nr52) << 4);
        #[cfg(feature = "apu-trace")]
        eprintln!("PCM12 read t={} -> {v:02X}", self.ticks_count);
        v
    }

    /// PCM34 ($FF77, CGB): current digital output of channels 3 (low nibble)
    /// and 4 (high nibble).
    #[inline(always)]
    pub fn read_pcm34(&self) -> u8 {
        self.ch3.get_sample(self.nr52) | (self.ch4.get_sample(self.nr52) << 4)
    }

    #[inline(always)]
    pub fn read(&self, address: u16) -> u8 {
        match address {
            CH1_START_ADDRESS..=CH1_END_ADDRESS => self.ch1.read(address),
            CH2_START_ADDRESS..=CH2_END_ADDRESS => self.ch2.read(address),
            CH3_START_ADDRESS..=CH3_END_ADDRESS => self.ch3.read(address),
            CH4_START_ADDRESS..=CH4_END_ADDRESS => self.ch4.read(address),
            AUDIO_MASTER_CONTROL_ADDRESS => self.nr52.read(),
            SOUND_PLANNING_ADDRESS => self.mixer.nr51_panning.byte,
            MASTER_VOLUME_ADDRESS => self.mixer.nr50_volume.byte,
            CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END => {
                self.ch3.wave_ram.read(address, self.nr52.is_ch3_on())
            }
            _ => {
                if (AUDIO_START_ADDRESS..=AUDIO_END_ADDRESS).contains(&address) {
                    return 0xFF;
                }

                panic!("Invalid APU address: {address:x}");
            }
        }
    }

    /// The frame sequencer generates low frequency clocks for the modulation
    /// units. It is clocked at 512 Hz by the falling edge of the DIV-APU bit
    /// (DIV bit 4, bit 5 in double speed) — so a DIV write that resets the
    /// counter while the bit is set produces an extra, early step
    /// (same-suite div_write_trigger*).
    #[inline(always)]
    fn sequence_frame(&mut self, div_apu_bit: bool) {
        let falling_edge = self.prev_div_apu_bit && !div_apu_bit;
        let rising_edge = !self.prev_div_apu_bit && div_apu_bit;
        self.prev_div_apu_bit = div_apu_bit;

        if !self.nr52.is_audio_on() {
            return;
        }

        // Secondary event: the rising edge between frame-sequencer events
        // arms the envelope clocks of channels with expired countdowns.
        if rising_edge {
            self.ch1.arm_envelope(&self.nr52);
            self.ch2.arm_envelope(&self.nr52);
            self.ch4.arm_envelope(&self.nr52);
            return;
        }

        if !falling_edge {
            return;
        }

        match self.skip_div_event {
            // The first edge after power-on with the DIV-APU bit set belongs
            // to the pre-power period and is swallowed entirely.
            SkipDivEvent::Skip => {
                self.skip_div_event = SkipDivEvent::Skipped;
                return;
            }
            // The next one runs, but the phase does not advance.
            SkipDivEvent::Skipped => self.skip_div_event = SkipDivEvent::None,
            SkipDivEvent::None => {
                self.frame_sequencer_step = self.frame_sequencer_step.wrapping_add(1) & 7;
            }
        }

        // 64 Hz: envelope countdowns run while their clock isn't armed.
        if self.frame_sequencer_step & 7 == 7 {
            self.ch1.countdown_envelope();
            self.ch2.countdown_envelope();
            self.ch4.countdown_envelope();
        }

        // Armed envelope clocks apply their volume step on every event.
        tick_envelope_some(self);

        if self.frame_sequencer_step & 1 == 1 {
            tick_length_all(self);
        }

        if self.frame_sequencer_step & 3 == 3 {
            let lf_odd = self.lf_ticks & 2 != 0;
            self.ch1.sweep_frame_event(lf_odd);
        }
    }
}

/// Phase   Length Ctr  Vol Env     Sweep
/// ---------------------------------------
/// odd     Clock       -           -
/// & 3==3  -           -           Clock
/// & 7==0  -           Clock       -
/// ---------------------------------------
/// Rate    256 Hz      64 Hz       128 Hz
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkipDivEvent {
    #[default]
    None,
    /// The next DIV-APU event is swallowed entirely.
    Skip,
    /// The event after a swallowed one runs without advancing the phase.
    Skipped,
}

#[inline(always)]
fn tick_length_all(apu: &mut Apu) {
    apu.ch1.tick_length(&mut apu.nr52);
    apu.ch2.tick_length(&mut apu.nr52);
    apu.ch3.tick_length(&mut apu.nr52);
    apu.ch4.tick_length(&mut apu.nr52);
}

#[inline(always)]
fn tick_envelope_some(apu: &mut Apu) {
    apu.ch1.tick_envelope();
    apu.ch2.tick_envelope();
    apu.ch4.tick_envelope();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One emulated second (4194304 APU ticks) must emit exactly
    /// `sample_rate` stereo pairs — the fractional accumulator may not
    /// truncate like the old integer `ticks % 95` did.
    #[test]
    fn test_exact_sample_rate() {
        // buffer larger than a second of stereo pairs so it never wraps
        let mut apu = Apu::new(ApuConfig::new(SAMPLING_FREQUENCY as usize * 2 + 2, 1.0));

        for _ in 0..CPU_CLOCK_SPEED {
            apu.tick(false);
        }

        assert_eq!(apu.get_buffer().len(), SAMPLING_FREQUENCY as usize * 2);
    }

    #[test]
    fn test_soft_clip() {
        // transparent-ish near zero, bounded at the extremes, odd-symmetric
        assert!((soft_clip(0.1) - 0.1).abs() < 0.001);
        assert!(soft_clip(2.0) <= 1.0);
        assert!(soft_clip(100.0) <= 1.0);
        assert_eq!(soft_clip(-0.5), -soft_clip(0.5));
    }

    /// All four channels running with their DACs on — the busy-game worst
    /// case for the per-tick mix path.
    fn make_busy_apu() -> Apu {
        let mut apu = Apu::new(ApuConfig::new(1 << 14, 1.0));
        apu.write(0xFF26, 0x80, false); // power on
        apu.write(0xFF25, 0xFF, false); // everything panned both sides
        apu.write(0xFF24, 0x77, false); // max master volume

        // wave RAM pattern (writable while ch3 is off)
        for (i, addr) in (CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END).enumerate() {
            apu.write(addr, (i as u8) << 4 | (15 - i as u8), false);
        }

        apu.write(0xFF12, 0xF0, false); // ch1 max volume, DAC on
        apu.write(0xFF13, 0x83, false);
        apu.write(0xFF14, 0x87, false); // trigger

        apu.write(0xFF17, 0xF0, false); // ch2
        apu.write(0xFF18, 0x21, false);
        apu.write(0xFF19, 0x86, false); // trigger

        apu.write(0xFF1A, 0x80, false); // ch3 DAC on
        apu.write(0xFF1C, 0x20, false); // full output level
        apu.write(0xFF1D, 0x00, false);
        apu.write(0xFF1E, 0x87, false); // trigger

        apu.write(0xFF21, 0xF0, false); // ch4
        apu.write(0xFF22, 0x44, false);
        apu.write(0xFF23, 0x80, false); // trigger

        apu
    }

    /// The cached mix must be invalidated by every input the mixer reads.
    /// Reference: the pre-cache code path — recompute the DAC+mix floats on
    /// every tick — run in lockstep against the keyed APU. The scenario
    /// covers every invalidation route: waveform steps, envelope decay and
    /// length expiry off the DIV-APU clock (NR52 changes outside `write`),
    /// mid-window NR50/NR51 writes, and a direct channel_mask config edit.
    /// Tolerance covers the float summation-order difference (per-tick adds
    /// vs cached-value-times-run-length).
    #[test]
    fn test_keyed_mix_matches_direct() {
        let mut apu = make_busy_apu();
        apu.write(0xFF12, 0xF4, false); // ch1 envelope: decreasing, pace 4
        apu.write(0xFF14, 0x87, false); // retrigger to reload it
        apu.write(0xFF16, 0x30, false); // ch2 length load 48
        apu.write(0xFF17, 0xF3, false); // ch2 envelope: decreasing, pace 3
        apu.write(0xFF19, 0xC6, false); // retrigger with length enabled

        let mut ref_hpf = Hpf::new(SAMPLING_FREQUENCY);
        let mut ref_mixer = apu.mixer.clone();
        let (mut ref_left, mut ref_right, mut ref_ticks) = (0.0f32, 0.0f32, 0u32);
        let mut ref_acc = apu.sample_acc;
        let mut emitted = 0usize;

        for i in 0..CPU_CLOCK_SPEED {
            match i {
                1_000_000 => apu.write(MASTER_VOLUME_ADDRESS, 0x34, false),
                2_000_000 => apu.write(SOUND_PLANNING_ADDRESS, 0xF0, false),
                3_000_000 => apu.config.channel_mask = 0b0101,
                _ => {}
            }

            let idx_before = apu.buffer_idx;
            // DIV bit driving the frame sequencer: 512 Hz falling edges
            apu.tick(i & (1 << 12) != 0);

            // Channel state after tick() is exactly what its mix observed.
            let (d1, s1) = apply_dac(apu.nr52, &apu.ch1);
            let (d2, s2) = apply_dac(apu.nr52, &apu.ch2);
            let (d3, s3) = apply_dac(apu.nr52, &apu.ch3);
            let (d4, s4) = apply_dac(apu.nr52, &apu.ch4);
            (ref_mixer.sample1, ref_mixer.sample2) = (s1, s2);
            (ref_mixer.sample3, ref_mixer.sample4) = (s3, s4);
            ref_mixer.nr50_volume.byte = apu.mixer.nr50_volume.byte;
            ref_mixer.nr51_panning.byte = apu.mixer.nr51_panning.byte;
            let (left, right) = ref_mixer.mix(apu.config.channel_mask);
            ref_left += left;
            ref_right += right;
            ref_ticks += 1;

            ref_acc += apu.sample_rate;
            if ref_acc >= CPU_CLOCK_SPEED {
                ref_acc -= CPU_CLOCK_SPEED;
                (ref_hpf.dac1_enabled, ref_hpf.dac2_enabled) = (d1, d2);
                (ref_hpf.dac3_enabled, ref_hpf.dac4_enabled) = (d3, d4);
                let scale = 1.0 / ref_ticks as f32;
                let (out_left, out_right) =
                    ref_hpf.apply_filter(ref_left * scale, ref_right * scale);
                (ref_left, ref_right, ref_ticks) = (0.0, 0.0, 0);

                assert_ne!(apu.buffer_idx, idx_before, "tick {i}: no pair emitted");
                let got_left = apu.buffer[apu.buffer_idx - 2];
                let got_right = apu.buffer[apu.buffer_idx - 1];
                assert!(
                    (got_left - out_left).abs() < 1e-4 && (got_right - out_right).abs() < 1e-4,
                    "tick {i}: keyed ({got_left}, {got_right}) vs direct ({out_left}, {out_right})"
                );
                emitted += 1;
            }
        }

        assert_eq!(emitted, SAMPLING_FREQUENCY as usize);
    }

    /// Throughput probe, not a pass/fail test: run with
    /// `cargo test -p core --release bench_tick -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_tick_throughput() {
        let mut apu = make_busy_apu();

        for _ in 0..CPU_CLOCK_SPEED {
            apu.tick(false); // warm-up second
        }

        const SECONDS: u32 = 4;
        let start = std::time::Instant::now();
        for _ in 0..SECONDS * CPU_CLOCK_SPEED {
            apu.tick(false);
        }
        let elapsed = start.elapsed();

        std::hint::black_box(&apu);
        let ticks = (SECONDS * CPU_CLOCK_SPEED) as f64;
        println!(
            "APU tick: {:.1} Mticks/s ({:.2} ns/tick, {:.1}x realtime)",
            ticks / elapsed.as_secs_f64() / 1e6,
            elapsed.as_nanos() as f64 / ticks,
            ticks / CPU_CLOCK_SPEED as f64 / elapsed.as_secs_f64(),
        );
    }

    #[test]
    fn test_sample_rate_clamp() {
        let mut apu = Apu::default();

        apu.set_sample_rate(0);
        assert_eq!(apu.sample_rate, SAMPLING_FREQUENCY - MAX_SAMPLE_RATE_SKEW);

        apu.set_sample_rate(u32::MAX);
        assert_eq!(apu.sample_rate, SAMPLING_FREQUENCY + MAX_SAMPLE_RATE_SKEW);
    }
}

/// FF26 — NR52: Audio master control
#[derive(Debug, Clone, Default, Copy, Serialize, Deserialize)]
pub struct NR52 {
    byte: u8,
}

impl NR52 {
    #[inline]
    pub fn write(&mut self, value: u8) {
        let prev_enabled = self.is_audio_on();
        let new_enabled = get_bit_flag(value, 7);

        if !new_enabled && prev_enabled {
            // APU is turning off, clear all but Wave RAM
            self.byte = 0;
        } else if new_enabled {
            // Turn on APU
            self.byte |= 0b1000_0000;
        }
    }

    #[inline]
    pub fn read(&self) -> u8 {
        self.byte | 0b0111_0000 // Bits 4-6 always read as 1
    }

    #[inline]
    pub fn is_audio_on(&self) -> bool {
        get_bit_flag(self.byte, 7)
    }

    #[inline]
    pub fn is_ch1_on(&self) -> bool {
        get_bit_flag(self.byte, Self::get_enable_bit_pos(ChannelType::CH1))
    }

    #[inline]
    pub fn is_ch2_on(&self) -> bool {
        get_bit_flag(self.byte, Self::get_enable_bit_pos(ChannelType::CH2))
    }

    #[inline]
    pub fn is_ch3_on(&self) -> bool {
        get_bit_flag(self.byte, Self::get_enable_bit_pos(ChannelType::CH3))
    }

    #[inline]
    pub fn is_ch4_on(&self) -> bool {
        get_bit_flag(self.byte, Self::get_enable_bit_pos(ChannelType::CH4))
    }

    /// Only the status of the channels’ generation circuits is reported
    #[inline]
    pub fn is_ch_on(&self, ch_type: ChannelType) -> bool {
        get_bit_flag(self.byte, Self::get_enable_bit_pos(ch_type))
    }

    #[inline]
    pub fn deactivate_ch(&mut self, ch_type: ChannelType) {
        set_bit(&mut self.byte, Self::get_enable_bit_pos(ch_type), false);
    }

    #[inline]
    pub fn deactivate_ch4(&mut self) {
        set_bit(
            &mut self.byte,
            Self::get_enable_bit_pos(ChannelType::CH4),
            false,
        );
    }

    #[inline]
    pub fn activate_ch1(&mut self) {
        set_bit(
            &mut self.byte,
            Self::get_enable_bit_pos(ChannelType::CH1),
            true,
        );
    }

    #[inline]
    pub fn activate_ch2(&mut self) {
        set_bit(
            &mut self.byte,
            Self::get_enable_bit_pos(ChannelType::CH2),
            true,
        );
    }

    #[inline]
    pub fn activate_ch3(&mut self) {
        set_bit(
            &mut self.byte,
            Self::get_enable_bit_pos(ChannelType::CH3),
            true,
        );
    }

    #[inline]
    pub fn activate_ch4(&mut self) {
        set_bit(
            &mut self.byte,
            Self::get_enable_bit_pos(ChannelType::CH4),
            true,
        );
    }

    #[inline]
    pub fn activate_ch(&mut self, ch_type: ChannelType) {
        set_bit(&mut self.byte, Self::get_enable_bit_pos(ch_type), true);
    }

    #[inline]
    pub fn on_dac_update(&mut self, dac_enabled: bool, ch_type: ChannelType) {
        // Disabling DAC always disables the channel
        if !dac_enabled {
            self.deactivate_ch(ch_type);
        }
    }

    #[inline]
    fn get_enable_bit_pos(ch_type: ChannelType) -> u8 {
        match ch_type {
            ChannelType::CH1 => 0,
            ChannelType::CH2 => 1,
            ChannelType::CH3 => 2,
            ChannelType::CH4 => 3,
        }
    }
}

/// FF25 — NR51:
/// Each channel can be panned hard left, center, hard right, or ignored entirely.
/// Setting a bit to 1 enables the channel to go into the selected output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NR51 {
    pub byte: u8,
}

impl NR51 {
    pub fn ch4_left(&self) -> bool {
        get_bit_flag(self.byte, 7)
    }
    pub fn ch3_left(&self) -> bool {
        get_bit_flag(self.byte, 6)
    }
    pub fn ch2_left(&self) -> bool {
        get_bit_flag(self.byte, 5)
    }
    pub fn ch1_left(&self) -> bool {
        get_bit_flag(self.byte, 4)
    }
    pub fn ch4_right(&self) -> bool {
        get_bit_flag(self.byte, 3)
    }
    pub fn ch3_right(&self) -> bool {
        get_bit_flag(self.byte, 2)
    }
    pub fn ch2_right(&self) -> bool {
        get_bit_flag(self.byte, 1)
    }
    pub fn ch1_right(&self) -> bool {
        get_bit_flag(self.byte, 0)
    }
}

/// FF24 — NR50: Master volume & VIN panning
/// A value of 0 is treated as a volume of 1 (very quiet), and a value of 7 is treated as a volume of 8 (no volume reduction). Importantly, the amplifier never mutes a non-silent input.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NR50 {
    pub byte: u8,
}

impl NR50 {
    pub fn left_volume(&self) -> u8 {
        (self.byte >> 4) & 0b111 // Extract bits 6-4
    }

    pub fn right_volume(&self) -> u8 {
        self.byte & 0b111 // Extract bits 2-0
    }

    pub fn vin_left_enabled(&self) -> bool {
        self.byte & 0b1000_0000 != 0
    }

    pub fn vin_right_enabled(&self) -> bool {
        self.byte & 0b0000_1000 != 0
    }
}

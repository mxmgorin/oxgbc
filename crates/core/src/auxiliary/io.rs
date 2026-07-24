use crate::apu::channels::wave_channel::{CH3_WAVE_RAM_END, CH3_WAVE_RAM_START};
use crate::apu::Apu;
use crate::apu::{AUDIO_END_ADDRESS, AUDIO_START_ADDRESS};
use crate::auxiliary::joypad::Joypad;
use crate::auxiliary::ram::{Ram, WRAM_BANK_NUMBER_ADDR};
use crate::auxiliary::timer::{Timer, TIMER_DIV_ADDRESS, TIMER_TAC_ADDRESS};
use crate::cpu::interrupts::{InterruptType, Interrupts};
use crate::emu::config::GbModel;
use crate::ppu::lcd::{
    CGB_BG_PALLETE_DATA_ADDR, CGB_OBJ_PALLETE_DATA_ADDR, CGB_OBJ_PRIORITY_MODE_ADDR,
    CGB_PALLETE_END_ADDR, CGB_PALLETE_START_ADDR, LCD_ADDRESS_END, LCD_ADDRESS_START,
};
use crate::ppu::vram::VRAM_BANK_NUMBER_ADDR;
use crate::ppu::Ppu;
use serde::{Deserialize, Serialize};

const IO_IF_UNUSED_MASK: u8 = 0b1110_0000;

// All unreadable bits of I/O registers return 1. In general, all unused bits in I/O registers are
// unreadable so they return 1. Some exceptions are:
// - Unknown purpose (if any) registers. Some bits of them can be read and written.
// - The IE register (only the 5 lower bits are used, but the upper 3 can hold any value).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Io {
    pub serial: Serial,
    pub timer: Timer,
    pub interrupts: Interrupts,
    pub joypad: Joypad,
    pub apu: Apu,
    pub ppu: Ppu,
    pub ram: Ram,
    pub cgb_speed: CgbSpeed,
    /// CGB undocumented registers FF72-FF75.
    #[serde(default)]
    pub undoc: CgbUndocumented,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CgbUndocumented {
    pub ff72: u8,
    pub ff73: u8,
    pub ff74: u8,
    /// Only bits 4-6 are stored; the rest read back as 1.
    pub ff75: u8,
}

impl Io {
    pub fn new(ppu: Ppu, apu: Apu) -> Self {
        Io {
            serial: Serial::default(),
            timer: Timer::default(),
            interrupts: Interrupts::default(),
            joypad: Default::default(),
            ram: Ram::default(),
            ppu,
            apu,
            cgb_speed: CgbSpeed::default(),
            undoc: CgbUndocumented::default(),
        }
    }

    #[inline(always)]
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joypad.get_byte(self.ppu.lcd.model),
            0xFF01 => self.serial.sb,
            0xFF02 => self.serial.sc | SERIAL_SC_UNUSED_MASK,
            TIMER_DIV_ADDRESS..=TIMER_TAC_ADDRESS => self.timer.read(addr),
            AUDIO_START_ADDRESS..=AUDIO_END_ADDRESS | CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END => {
                self.apu.read(addr)
            }
            LCD_ADDRESS_START..=LCD_ADDRESS_END => self.ppu.lcd.read(addr),
            // KEY1 is disabled in DMG-compat mode on CGB.
            0xFF4D => {
                if self.ppu.lcd.is_cgb_mode() {
                    self.cgb_speed.read()
                } else {
                    0xFF
                }
            }
            CGB_OBJ_PRIORITY_MODE_ADDR => {
                if self.ppu.lcd.is_cgb_mode() {
                    self.ppu.lcd.read_obj_priority_mode()
                } else {
                    0xFF
                }
            }
            VRAM_BANK_NUMBER_ADDR
            | 0xFF50
            | 0xFF51..=0xFF55
            | CGB_PALLETE_START_ADDR..=CGB_PALLETE_END_ADDR
            | WRAM_BANK_NUMBER_ADDR => match self.ppu.lcd.model {
                GbModel::Dmg => 0xFF,
                GbModel::Cgb => match addr {
                    // VBK responds on CGB hardware even in DMG-compat mode
                    // (locked to bank 0 there).
                    VRAM_BANK_NUMBER_ADDR => self.ppu.video_ram.read_bank_number(),
                    WRAM_BANK_NUMBER_ADDR => {
                        if self.ppu.lcd.is_cgb_mode() {
                            self.ram.read_wram_bank()
                        } else {
                            0xFF
                        }
                    }
                    CGB_PALLETE_START_ADDR..=CGB_PALLETE_END_ADDR => {
                        // The data ports (BCPD/OCPD) are inaccessible during
                        // mode 3 and in DMG-compat mode; the BCPS/OCPS index
                        // registers stay readable on CGB hardware.
                        if matches!(addr, CGB_BG_PALLETE_DATA_ADDR | CGB_OBJ_PALLETE_DATA_ADDR)
                            && (self.ppu.lcd.is_vram_blocked() || !self.ppu.lcd.is_cgb_mode())
                        {
                            return 0xFF;
                        }

                        self.ppu.lcd.cgb_palette.read(addr)
                    }
                    _ => 0xFF,
                },
            },
            // CGB undocumented registers; present in DMG-compat mode too,
            // except FF74 which is CGB-mode only.
            0xFF72 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff72,
            0xFF73 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff73,
            0xFF74 if self.ppu.lcd.is_cgb_mode() => self.undoc.ff74,
            0xFF75 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff75 | 0x8F,
            // PCM12 / PCM34: current digital output of the APU channels.
            0xFF76 if self.ppu.lcd.model == GbModel::Cgb => self.apu.read_pcm12(),
            0xFF77 if self.ppu.lcd.model == GbModel::Cgb => self.apu.read_pcm34(),
            0xFF0F => self.interrupts.int_flags | IO_IF_UNUSED_MASK,
            _ => 0xFF,
        }
    }

    #[inline(always)]
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF00 => self.joypad.set_byte(value),
            0xFF01 => self.serial.sb = value,
            0xFF02 => {
                // Seed the edge detector as continuous tracking would have
                // left it (the pre-write clock selection).
                let clk = self.timer.serial_clock_bit(self.serial.is_fast_clock());
                self.serial.write_sc(value, clk);
            }
            TIMER_DIV_ADDRESS..=TIMER_TAC_ADDRESS => self.timer.write(addr, value),
            AUDIO_START_ADDRESS..=AUDIO_END_ADDRESS | CH3_WAVE_RAM_START..=CH3_WAVE_RAM_END => {
                self.apu.write(addr, value, self.cgb_speed.double_speed)
            }
            LCD_ADDRESS_START..=LCD_ADDRESS_END => {
                self.ppu.write_lcd(addr, value, &mut self.interrupts)
            }
            0xFF4D => {
                if self.ppu.lcd.is_cgb_mode() {
                    self.cgb_speed.write(value)
                }
            }
            CGB_OBJ_PRIORITY_MODE_ADDR => {
                if self.ppu.lcd.is_cgb_mode() {
                    self.ppu.lcd.write_obj_priority_mode(value)
                }
            }
            VRAM_BANK_NUMBER_ADDR
            | 0xFF50
            | 0xFF51..=0xFF55
            | CGB_PALLETE_START_ADDR..=CGB_PALLETE_END_ADDR
            | WRAM_BANK_NUMBER_ADDR => match self.ppu.lcd.model {
                GbModel::Cgb => match addr {
                    VRAM_BANK_NUMBER_ADDR => {
                        if self.ppu.lcd.is_cgb_mode() {
                            self.ppu.video_ram.write_bank_number(value)
                        }
                    }
                    WRAM_BANK_NUMBER_ADDR => {
                        if self.ppu.lcd.is_cgb_mode() {
                            self.ram.write_wram_bank(value)
                        }
                    }
                    CGB_PALLETE_START_ADDR..=CGB_PALLETE_END_ADDR => {
                        // Data ports (BCPD/OCPD) are disabled in DMG-compat mode.
                        if matches!(addr, CGB_BG_PALLETE_DATA_ADDR | CGB_OBJ_PALLETE_DATA_ADDR)
                            && !self.ppu.lcd.is_cgb_mode()
                        {
                            return;
                        }

                        if self.ppu.lcd.is_vram_blocked() {
                            // During mode 3 a data-port (BCPD/OCPD) write is dropped
                            // but still advances the auto-increment index; index-port
                            // (BCPS/OCPS) writes stay allowed.
                            match addr {
                                CGB_BG_PALLETE_DATA_ADDR | CGB_OBJ_PALLETE_DATA_ADDR => {
                                    self.ppu.lcd.cgb_palette.tick_index_on_blocked_write(addr);
                                }
                                _ => self.ppu.lcd.cgb_palette.write(addr, value),
                            }

                            return;
                        }

                        self.ppu.lcd.cgb_palette.write(addr, value)
                    }
                    _ => {}
                },
                _ => {}
            },
            0xFF72 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff72 = value,
            0xFF73 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff73 = value,
            0xFF74 if self.ppu.lcd.is_cgb_mode() => self.undoc.ff74 = value,
            0xFF75 if self.ppu.lcd.model == GbModel::Cgb => self.undoc.ff75 = value & 0x70,
            0xFF0F => self.interrupts.int_flags = value,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Serial {
    /// FF01 — SB: Serial transfer data
    sb: u8,
    /// FF02 — SC: Serial transfer control
    sc: u8,
    /// T-cycles left in the in-progress transfer (0 = idle).
    /// Bits left in the in-progress transfer (0 = idle).
    #[serde(default)]
    bits_left: u8,
    /// Previous state of the serial clock bit, for edge detection.
    #[serde(default)]
    prev_clock: bool,
    /// Byte latched when a transfer starts, for the debug serial log. Kept
    /// independent of the transfer timing so blargg's text output is captured
    /// even when the ROM fires bytes back-to-back without waiting.
    output: Option<u8>,
}

const SERIAL_SC_UNUSED_MASK: u8 = 0b01111110;

impl Serial {
    /// Write SC ($FF02). Starting a transfer requires the transfer bit (7) and
    /// the internal clock (bit 0); bit 1 selects the CGB fast clock.
    /// `clock_bit` is the current serial clock bit: `tick` is skipped while no
    /// transfer runs, so the idle edge detector is re-seeded here instead.
    #[inline]
    pub fn write_sc(&mut self, value: u8, clock_bit: bool) {
        self.prev_clock = clock_bit;
        self.sc = value;

        if value & 0x81 == 0x81 {
            self.output = Some(self.sb);
            self.bits_left = 8;
        }
    }

    /// A transfer is in progress — the only time `tick` can do anything.
    #[inline(always)]
    pub fn is_active(&self) -> bool {
        self.bits_left != 0
    }

    /// The CGB fast clock (SC bit 1) is selected for the active transfer.
    #[inline(always)]
    pub fn is_fast_clock(&self) -> bool {
        self.sc & 0x02 != 0
    }

    /// Advance one T-cycle. The serial clock is divided from the same
    /// free-running counter as DIV, so bit shifts align to edges anchored at
    /// reset time, not at the SC write (mooneye boot_sclk_align): one bit per
    /// falling edge of `clock_bit`. On completion the CPU with no link partner
    /// has shifted in all 1s, the transfer bit clears, and the serial
    /// interrupt is requested.
    #[inline(always)]
    pub fn tick(&mut self, clock_bit: bool, interrupts: &mut Interrupts) {
        let falling = self.prev_clock && !clock_bit;
        self.prev_clock = clock_bit;

        if self.bits_left == 0 || !falling {
            return;
        }

        self.sb = (self.sb << 1) | 1;
        self.bits_left -= 1;

        if self.bits_left == 0 {
            self.sc &= 0x7F;
            interrupts.request_interrupt(InterruptType::Serial);
        }
    }

    /// Batch equivalent of `ticks × tick(bit_at(t), …)` for an active transfer.
    /// The serial clock bit comes from the free-running DIV counter (`div0` =
    /// its window-start value), so it only changes at mask crossings; ticks
    /// between them are no-ops. Only the transition ticks are replayed through
    /// the real `tick`, keeping edge semantics (mooneye boot_sclk_align) intact.
    pub fn advance(&mut self, div0: u16, ticks: usize, interrupts: &mut Interrupts) {
        let mask = if self.is_fast_clock() { 1u16 << 3 } else { 1u16 << 8 };
        let period = mask as usize;

        // A DIV write mid-transfer shifts the counter phase: the latch then
        // disagrees with the bit level and the next tick sees an edge the phase
        // math below would never place (a DIV reset clocks serial on hardware,
        // like the TIMA DIV-write glitch). Replay that tick for real; if it
        // coincides with a regular transition, the later loop call is a no-op.
        if ticks > 0 && self.bits_left != 0 {
            let bit1 = div0.wrapping_add(1) & mask != 0;
            if self.prev_clock != bit1 {
                self.tick(bit1, interrupts);
            }
        }

        let mut offset = period - (div0 as usize & (period - 1));
        while offset <= ticks && self.bits_left != 0 {
            let bit = div0.wrapping_add(offset as u16) & mask != 0;
            self.tick(bit, interrupts);
            offset += period;
        }
    }

    #[inline(always)]
    pub fn has_data(&self) -> bool {
        self.output.is_some()
    }

    #[inline(always)]
    pub fn take_data(&mut self) -> u8 {
        self.output.take().unwrap_or(0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IoAddress {
    Joypad,
    /// FF01 — SB: Serial transfer data
    SerialSb,
    /// FF02 — SC: Serial transfer control
    SerialSc,
    Timer,
    InterruptFlags,
    Audio,
    WavePattern,
    Display,
    VRAMBankSelect,
    DisableBootROM,
    VRAMdma,
    Background,
    WRAMBankSelect,
    Unused,
}

/// KEY1/SPD (CGB Mode only): Prepare speed switch
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CgbSpeed {
    pub double_speed: bool,   // bit 7
    pub prepare_switch: bool, // bit 0
}

impl CgbSpeed {
    pub fn toggle(&mut self) {
        // Only toggle if a switch was prepared (KEY1 bit 0)
        if !self.prepare_switch {
            return;
        }

        self.double_speed = !self.double_speed;
        self.prepare_switch = false;
    }

    pub fn read(&self) -> u8 {
        let mut value = 0x7E; // bits 1–6 always read as 1

        if self.double_speed {
            value |= 0x80; // bit 7
        }

        if self.prepare_switch {
            value |= 0x01; // bit 0
        }

        value
    }

    pub fn write(&mut self, value: u8) {
        // Only bit 0 is writable
        self.prepare_switch = (value & 0x01) != 0;
    }
}

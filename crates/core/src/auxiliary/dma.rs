use crate::bus::{Bus, ECHO_MIRROR_OFFSET};
use crate::get_bit_flag;
use crate::ppu::lcd::PpuMode;
use crate::ppu::oam::OAM_ADDR_START;
use crate::ppu::vram::{VRAM_ADDR_END, VRAM_ADDR_START};
use serde::{Deserialize, Serialize};

pub const VRAM_DMA_ADDR_START: u16 = 0xFF51;
pub const VRAM_DMA_ADDR_END: u16 = 0xFF55;
const VRAM_DMA_CHUNK_SIZE: u8 = 16;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OamDma {
    pub is_active: bool,
    pub current_index: u16,
    pub src_addr: u16,
    pub start_delay: u8,
    pub queue_addr: Option<u16>,
}

impl OamDma {
    #[inline]
    pub fn start(&mut self, address: u8) {
        if self.is_active {
            self.queue_addr = Some((address as u16) << 8);
        } else {
            self.src_addr = (address as u16) << 8;
            self.current_index = 0;
        }

        self.start_delay = 2;
        self.is_active = true;
    }

    #[inline]
    pub fn is_transferring(&self) -> bool {
        self.is_active && (self.start_delay == 0 || self.queue_addr.is_some())
    }

    /// Executes a single OAM DMA write and auto-increments the internal index cursor.
    #[inline]
    pub fn tick(bus: &mut Bus) {
        if !bus.oam_dma.is_active {
            return;
        }

        if bus.oam_dma.start_delay > 0 {
            bus.oam_dma.start_delay -= 1;

            if bus.oam_dma.queue_addr.is_none() {
                return;
            }
        } else if let Some(queue_addr) = bus.oam_dma.queue_addr {
            bus.oam_dma.queue_addr = None;
            bus.oam_dma.src_addr = queue_addr;
            bus.oam_dma.current_index = 0;
        }

        let addr = bus.oam_dma.src_addr + bus.oam_dma.current_index;
        // DMA from high addresses doesn't read from HRAM or MMIO, it reads an extended echo ram instead
        let addr = match addr {
            0xFE00..=0xFFFF => addr - ECHO_MIRROR_OFFSET,
            _ => addr,
        };
        let byte = bus.read(addr);
        let dest_addr = OAM_ADDR_START + bus.oam_dma.current_index;
        bus.io.ppu.oam_ram.write(dest_addr, byte);
        bus.oam_dma.current_index = bus.oam_dma.current_index.wrapping_add(1);
        bus.oam_dma.is_active = bus.oam_dma.current_index < 160;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VramDmaState {
    #[default]
    Idle,
    GpDmaTransferring,
    HDmaTransferring {
        chunk_bytes: u8,
    },
    WaitingHBlank,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VramDma {
    src_addr: u16,
    dst_addr: u16,
    state: VramDmaState,
    pending_bytes: u16,
    prev_ppu_mode: PpuMode,

    // Registers
    hdma1: u8,
    hdma2: u8,
    hdma3: u8,
    hdma4: u8,
}

impl VramDma {
    /// No transfer, no HBlank wait; the clock skips the
    /// 2 MHz tick entirely. Leaving Idle takes an HDMA5 write (between
    /// windows). `prev_ppu_mode` may go stale while skipped: harmless, since
    /// `WaitingHBlank` is only entered with mode != HBlank and the first
    /// waiting tick re-seeds it before a transition can be seen.
    #[inline(always)]
    pub fn is_idle(&self) -> bool {
        self.state == VramDmaState::Idle
    }

    pub fn is_transferring(&self) -> bool {
        matches!(
            self.state,
            VramDmaState::GpDmaTransferring | VramDmaState::HDmaTransferring { .. }
        )
    }

    #[inline(always)]
    pub fn tick(bus: &mut Bus) {
        let current_ppu_mode = bus.io.ppu.lcd.status.get_ppu_mode();
        let prev_ppu_mode = bus.vram_dma.prev_ppu_mode;
        bus.vram_dma.prev_ppu_mode = current_ppu_mode;

        match bus.vram_dma.state {
            VramDmaState::Idle => return,
            VramDmaState::WaitingHBlank => {
                if prev_ppu_mode != PpuMode::HBlank && current_ppu_mode == PpuMode::HBlank {
                    // Reached HBlank
                    bus.vram_dma.state = VramDmaState::HDmaTransferring {
                        chunk_bytes: VRAM_DMA_CHUNK_SIZE,
                    };
                    VramDma::transfer_byte(bus);
                }
            }
            VramDmaState::HDmaTransferring { .. } | VramDmaState::GpDmaTransferring => {
                VramDma::transfer_byte(bus);
            }
        }
    }

    #[inline]
    pub fn write(&mut self, addr: u16, value: u8, ppu_mode: PpuMode) {
        match addr {
            0xFF51 => {
                self.hdma1 = value;
                self.src_addr = self.compute_src();
            }
            0xFF52 => {
                self.hdma2 = value & 0xF0;
                self.src_addr = self.compute_src();
            }
            0xFF53 => {
                self.hdma3 = value & 0x1F;
                self.dst_addr = self.compute_dst();
            }
            0xFF54 => {
                self.hdma4 = value & 0xF0;
                self.dst_addr = self.compute_dst();
            }
            0xFF55 => self.write_hdma5(value, ppu_mode),
            _ => {}
        }
    }

    #[inline]
    pub fn read_hdma5(&self) -> u8 {
        let chunks = self.pending_bytes / VRAM_DMA_CHUNK_SIZE as u16;
        let length = (chunks as u8).wrapping_sub(1) & 0x7F;
        let status = u8::from(self.state == VramDmaState::Idle);

        length | (status << 7)
    }

    #[inline]
    fn write_hdma5(&mut self, value: u8, ppu_mode: PpuMode) {
        let length = u16::from((value & 0x7F) + 1);
        let bytes = VRAM_DMA_CHUNK_SIZE as u16 * length;
        let is_bit7_set = get_bit_flag(value, 7);

        // The length field is stored on EVERY write — including the one that
        // pauses a waiting HBlank transfer, so a paused HDMA5 reads back the
        // written length (same-suite hdma_lcd_off expects $00 -> $80).
        // The CPU is stalled while bytes actually move, so a write can only
        // land in Idle or WaitingHBlank.
        self.pending_bytes = bytes;

        if !is_bit7_set && self.state == VramDmaState::WaitingHBlank {
            // Pause the pending HBlank transfer.
            self.state = VramDmaState::Idle;
            return;
        }

        if is_bit7_set {
            if ppu_mode == PpuMode::HBlank {
                // Transfer immediately when HDMA is started on HBlank (this
                // includes the LCD-off state, where the mode reads 0).
                self.state = VramDmaState::HDmaTransferring {
                    chunk_bytes: VRAM_DMA_CHUNK_SIZE,
                };
            } else {
                self.state = VramDmaState::WaitingHBlank;
            }
        } else {
            self.state = VramDmaState::GpDmaTransferring;
        }
    }

    #[inline(always)]
    fn transfer_byte(bus: &mut Bus) {
        let byte = match bus.vram_dma.src_addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => bus.cart.read(bus.vram_dma.src_addr),
            0xC000..=0xDFFF => bus.io.ram.read_wram(bus.vram_dma.src_addr),
            // not accessible from VRAM DMA
            VRAM_ADDR_START..=VRAM_ADDR_END | 0xE000..=0xFFFF => 0xFF,
        };
        bus.vram_dma.src_addr = bus.vram_dma.src_addr.wrapping_add(1);

        VramDma::write_byte(bus, byte);
    }

    #[inline(always)]
    fn write_byte(bus: &mut Bus, byte: u8) {
        if bus.vram_dma.dst_addr > VRAM_ADDR_END {
            bus.vram_dma.dst_addr = VRAM_ADDR_START
        }

        if !bus.io.ppu.lcd.is_vram_blocked() {
            bus.io.ppu.video_ram.write(bus.vram_dma.dst_addr, byte);
        }

        // todo: overflow check should be on full byte
        let (new_dst, dst_overflowed) = bus.vram_dma.dst_addr.overflowing_add(1);
        bus.vram_dma.dst_addr = new_dst;

        if dst_overflowed {
            bus.vram_dma.state = VramDmaState::Idle;
            return;
        }

        bus.vram_dma.pending_bytes -= 1;

        if let VramDmaState::HDmaTransferring { chunk_bytes } = &mut bus.vram_dma.state {
            *chunk_bytes -= 1;

            if *chunk_bytes == 0 {
                bus.vram_dma.state = VramDmaState::WaitingHBlank;
            }
        }

        if bus.vram_dma.pending_bytes == 0 {
            bus.vram_dma.state = VramDmaState::Idle;
        }
    }

    #[inline(always)]
    fn compute_src(&self) -> u16 {
        ((self.hdma1 as u16) << 8) | ((self.hdma2 as u16) & 0xF0)
    }

    #[inline(always)]
    fn compute_dst(&self) -> u16 {
        0x8000 | (((self.hdma3 as u16) & 0x1F) << 8) | ((self.hdma4 as u16) & 0xF0)
    }
}

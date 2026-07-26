//! `Ppu::advance` must be dot-identical to the per-tick `tick()` chain —
//! driven with a randomized register-write script (LCDC on/off cycling,
//! STAT/LYC/scroll/window writes, OAM traffic feeding the sprite scan)
//! between windows of random instruction-like size. Observables compared
//! after every window: the LCD register file, the access-blocking flags,
//! the IF bits and the frame counter; the full serialized PPU state
//! (framebuffer, fetcher, VRAM included) is compared periodically and at
//! the end of every seed.

use core::cpu::interrupts::Interrupts;
use core::ppu::lcd::{
    LCD_CONTROL_ADDRESS, LCD_LY_ADDRESS, LCD_LY_COMPARE_ADDRESS, LCD_SCROLL_X_ADDRESS,
    LCD_SCROLL_Y_ADDRESS, LCD_STATUS_ADDRESS, LCD_WINDOW_X_ADDRESS, LCD_WINDOW_Y_ADDRESS,
};
use core::ppu::oam::{OAM_ADDR_START, OAM_ENTRIES_COUNT};
use core::ppu::Ppu;

mod common;
use common::Lcg;

const OAM_BYTES: u32 = OAM_ENTRIES_COUNT as u32 * 4; // 4 bytes per entry

// LY (read-only), DMA and the DMG palettes are excluded: no PPU timing effect.
const WRITE_ADDRS: [u16; 7] = [
    LCD_CONTROL_ADDRESS,
    LCD_STATUS_ADDRESS,
    LCD_SCROLL_Y_ADDRESS,
    LCD_SCROLL_X_ADDRESS,
    LCD_LY_COMPARE_ADDRESS,
    LCD_WINDOW_Y_ADDRESS,
    LCD_WINDOW_X_ADDRESS,
];

// WRITE_ADDRS plus LY: read-only, so never written, but its value is compared.
const READ_ADDRS: [u16; 8] = [
    LCD_CONTROL_ADDRESS,
    LCD_STATUS_ADDRESS,
    LCD_SCROLL_Y_ADDRESS,
    LCD_SCROLL_X_ADDRESS,
    LCD_LY_ADDRESS,
    LCD_LY_COMPARE_ADDRESS,
    LCD_WINDOW_Y_ADDRESS,
    LCD_WINDOW_X_ADDRESS,
];

#[test]
fn test_ppu_advance_equals_ticks_randomized() {
    for seed in 0..8u64 {
        let mut rng = Lcg::seeded(seed);
        let mut a = Ppu::default(); // reference: per-tick
        let mut b = Ppu::default(); // candidate: advance
        let mut ia = Interrupts::default();
        let mut ib = Interrupts::default();

        // ~5 frames per seed at the mean window size
        for round in 0..60_000usize {
            if rng.below(8) == 0 {
                for _ in 0..=rng.below(2) {
                    let addr = WRITE_ADDRS[rng.below(WRITE_ADDRS.len() as u32) as usize];
                    // Keep the LCD on most of the time so the frame actually
                    // progresses; bit 7 still cycles often enough to exercise
                    // the lcdon_line0 path.
                    let val = if addr == LCD_CONTROL_ADDRESS && rng.below(4) != 0 {
                        rng.below(256) as u8 | 0x80
                    } else {
                        rng.below(256) as u8
                    };
                    a.write_lcd(addr, val, &mut ia);
                    b.write_lcd(addr, val, &mut ib);
                }
            }
            if rng.below(16) == 0 {
                // OAM traffic: moves sprites across scanlines so the dot-80
                // scan and the mode-3 sprite stalls see fresh input
                let addr = OAM_ADDR_START + rng.below(OAM_BYTES) as u16;
                let val = rng.below(256) as u8;
                a.oam_ram.write(addr, val);
                b.oam_ram.write(addr, val);
            }

            let w = (rng.below(24) + 1) as usize;
            for _ in 0..w {
                a.tick(&mut ia);
            }
            b.advance(w, &mut ib);

            assert_eq!(ia.int_flags, ib.int_flags, "IF seed={seed} round={round}");
            assert_eq!(
                a.current_frame, b.current_frame,
                "frame seed={seed} round={round}"
            );
            for addr in READ_ADDRS {
                assert_eq!(
                    a.lcd.read(addr),
                    b.lcd.read(addr),
                    "reg {addr:04x} seed={seed} round={round}"
                );
            }
            assert_eq!(
                a.lcd.is_oam_read_blocked(),
                b.lcd.is_oam_read_blocked(),
                "oam read block seed={seed} round={round}"
            );
            assert_eq!(
                a.lcd.is_oam_write_blocked(),
                b.lcd.is_oam_write_blocked(),
                "oam write block seed={seed} round={round}"
            );
            assert_eq!(
                a.lcd.is_vram_read_blocked(),
                b.lcd.is_vram_read_blocked(),
                "vram read block seed={seed} round={round}"
            );

            if round % 5_000 == 0 {
                assert_eq!(
                    serde_json::to_string(&a).unwrap(),
                    serde_json::to_string(&b).unwrap(),
                    "full state seed={seed} round={round}"
                );
            }
        }

        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap(),
            "full state seed={seed} end"
        );
    }
}

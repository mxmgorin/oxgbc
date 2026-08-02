use crate::emu::config::GbModel;
use crate::ppu::lcd::{Lcd, PixelColor};
use crate::ppu::oam::{OamEntry, OamRam};
use crate::ppu::tile::{
    get_color_id, TileFlags, TileLineData, TILE_BIT_SIZE, TILE_LINE_BYTES_COUNT,
    TILE_SET_DATA_1_START,
};
use crate::ppu::vram::VideoRam;
use serde::{Deserialize, Serialize};

const MAX_LINE_SPRITES_COUNT: usize = 10;
const MAX_FETCHED_SPRITES_COUNT: usize = 4;

#[derive(Debug, Clone, Default, Copy, Serialize, Deserialize)]
pub struct SpriteFetchedData {
    pub tile_line: TileLineData,
    pub oam: OamEntry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpriteFetcher {
    line_sprites_count: usize,
    line_sprites: [OamEntry; MAX_LINE_SPRITES_COUNT],
    fetched_sprites_count: usize,
    fetched_sprites: [SpriteFetchedData; MAX_FETCHED_SPRITES_COUNT],
    /// Sprites (by `line_sprites` index) whose mode-3 fetch penalty has
    /// already been paid on this scanline.
    #[serde(default)]
    penalty_paid: u16,
}

impl SpriteFetcher {
    #[inline(always)]
    pub fn scan_oam(&mut self, lcd: &Lcd, oam_ram: &OamRam) {
        let mut line_sprites_count = 0;
        let cur_y = lcd.ly.wrapping_add(16);
        let sprite_height = lcd.control.get_obj_height();
        self.penalty_paid = 0;

        for ram_entry in oam_ram.entries.iter() {
            // Note: X = 0 sprites are fully offscreen but still occupy a slot
            // in the 10-sprite line limit and still stall the fetcher.

            // Check if the sprite is on the current scanline
            if ram_entry.y <= cur_y && ram_entry.y + sprite_height > cur_y {
                let mut inserted = false;

                // No sorting in CGB mode
                if lcd.is_dmg_obj_priority_mode() {
                    // Iterate through sorted list to insert at correct position
                    for i in 0..line_sprites_count {
                        let added_entry = unsafe { self.line_sprites.get_unchecked(i) };

                        if ram_entry.x < added_entry.x && ram_entry.x != added_entry.x {
                            // Sort by X first, then by OAM index if X is the same
                            for j in (i..line_sprites_count).rev() {
                                unsafe {
                                    *self.line_sprites.get_unchecked_mut(j + 1) =
                                        *self.line_sprites.get_unchecked(j)
                                }
                            }

                            unsafe { *self.line_sprites.get_unchecked_mut(i) = *ram_entry };
                            inserted = true;
                            break;
                        }
                    }
                }

                if !inserted {
                    // If no earlier insertion, push to the back
                    unsafe {
                        *self.line_sprites.get_unchecked_mut(line_sprites_count) = *ram_entry;
                    }
                }

                line_sprites_count += 1;
            }

            if line_sprites_count >= MAX_LINE_SPRITES_COUNT {
                break;
            }
        }

        self.line_sprites_count = line_sprites_count;
    }

    #[inline(always)]
    pub fn fetch(&mut self, lcd: &Lcd, vram: &VideoRam, scroll_x: u8, fetch_x: u8) {
        let mut fetched_sprites_count = 0;
        let cur_y = lcd.ly.wrapping_add(TILE_BIT_SIZE as u8);
        let sprite_height = lcd.control.get_obj_height();
        // Bits 0-3 of an OAM flag byte are CGB attributes, and a DMG game leaves
        // whatever it likes in them — ZAS sets bit 3 on every sprite. Outside CGB
        // mode the hardware ignores them and takes the tile from bank 0, which is
        // the only bank such a game ever wrote.
        let cgb_attrs = lcd.is_cgb_mode();

        for i in 0..self.line_sprites_count {
            let oam = unsafe { *self.line_sprites.get_unchecked(i) };
            let sp_x = self.calc_sprite_x(oam.x, scroll_x);

            if (sp_x >= fetch_x && sp_x < fetch_x.wrapping_add(8))
                || (sp_x.wrapping_add(8) >= fetch_x
                    && sp_x.wrapping_add(8) < fetch_x.wrapping_add(8))
            {
                // need to add
                let mut tile_y = cur_y
                    .wrapping_sub(oam.y)
                    .wrapping_mul(TILE_LINE_BYTES_COUNT as u8);

                if oam.flags.is_y_flip() {
                    tile_y = sprite_height
                        .wrapping_mul(2)
                        .wrapping_sub(2)
                        .wrapping_sub(tile_y);
                }

                let tile_index = if sprite_height == 16 {
                    // remove last bit
                    oam.tile_index & !1
                } else {
                    oam.tile_index
                };

                let addr = TILE_SET_DATA_1_START
                    .wrapping_add(tile_index as u16 * TILE_BIT_SIZE)
                    .wrapping_add(tile_y as u16);
                let vram_bank = if cgb_attrs {
                    oam.flags.read_cgb_vram_bank()
                } else {
                    0
                };
                let tile_line = vram.read_tile_line_from_bank(vram_bank, addr);

                unsafe {
                    let sprite = self
                        .fetched_sprites
                        .get_unchecked_mut(fetched_sprites_count);
                    sprite.oam = oam;
                    sprite.tile_line = tile_line;
                };
                fetched_sprites_count += 1;
            }

            if fetched_sprites_count >= MAX_FETCHED_SPRITES_COUNT {
                // max 3 sprites on pixels
                break;
            }
        }

        self.fetched_sprites_count = fetched_sprites_count;
    }

    #[inline(always)]
    fn calc_sprite_x(&self, sprite_x: u8, scroll_x: u8) -> u8 {
        sprite_x.wrapping_sub(8).wrapping_add(scroll_x % 8)
    }

    /// Mode-3 sprite fetch penalty for the pixel about to be output at `x`.
    /// Each sprite reaching its first visible pixel stalls the fetcher for
    /// 6 dots; the first sprite of a batch pays an extra background-fetch
    /// alignment penalty of `5 - min(5, (X + SCX) % 8)` dots, and the first
    /// stall of the scanline overlaps the fetcher warm-up by 3 dots. Sprites
    /// with OAM X < 8 (including the hidden X = 0) trigger at pixel 0.
    /// Calibrated against the full mooneye intr_2_mode0_timing_sprites table.
    #[inline(always)]
    pub fn take_penalty(&mut self, x: u8, scroll_x: u8) -> u8 {
        if self.line_sprites_count == 0 {
            return 0;
        }

        let first_of_line = self.penalty_paid == 0;
        let mut penalty = 0u8;
        // A batch = consecutive sprites sharing one OAM X; each batch pays the
        // alignment part once (sprites at X and X+8 both trigger at pixel 0
        // but are separate batches).
        let mut batch_x = None;

        for i in 0..self.line_sprites_count {
            if self.penalty_paid & (1 << i) != 0 {
                continue;
            }

            let oam = unsafe { self.line_sprites.get_unchecked(i) };

            if oam.x.saturating_sub(8) != x {
                continue;
            }

            self.penalty_paid |= 1 << i;
            penalty += 6;

            if batch_x != Some(oam.x) {
                batch_x = Some(oam.x);
                let align = (oam.x.wrapping_add(scroll_x)) % 8;
                penalty += 5u8.saturating_sub(align);
            }
        }

        if first_of_line && penalty > 0 {
            penalty -= 3;
        }

        penalty
    }

    #[inline(always)]
    pub fn get_color(
        &self,
        lcd: &Lcd,
        fifo_x: u8,
        bg_color_id: usize,
        bg_flags: TileFlags,
    ) -> Option<PixelColor> {
        if !lcd.control.is_obj_enabled() {
            return None;
        }

        let scroll_x = lcd.scroll_x;

        for i in 0..self.fetched_sprites_count {
            let obj = unsafe { self.fetched_sprites.get_unchecked(i) };
            let sprite_x = self.calc_sprite_x(obj.oam.x, scroll_x);

            let offset = fifo_x.wrapping_sub(sprite_x);
            if !(0..=7).contains(&offset) {
                continue; // Out of sprite range
            }

            let bit = if obj.oam.flags.is_x_flip() {
                7 - offset
            } else {
                offset
            };

            let color_index = get_color_id(obj.tile_line.byte0, obj.tile_line.byte1, bit);

            if is_show_obj(lcd, bg_color_id, bg_flags, color_index, obj.oam.flags) {
                let color = lcd.get_obj_color(obj.oam.flags, color_index);
                return Some(color);
            }
        }

        None
    }
}

#[inline(always)]
pub fn is_show_obj(
    lcd: &Lcd,
    bg_color_index: usize,
    bg_flags: TileFlags,
    obj_color_index: usize,
    obj_flags: TileFlags,
) -> bool {
    // Transparent
    if obj_color_index == 0 {
        return false;
    }

    // BG color 0 is always transparent for priority
    if bg_color_index == 0 {
        return true;
    }

    match lcd.model {
        GbModel::Dmg => {
            if obj_flags.is_bgw_priority() {
                return false;
            }

            true
        }

        // In CGB mode:
        // If the BG color index is 0, the OBJ will always have priority;
        // If LCDC bit 0 is clear, the OBJ will always have priority;
        // If both the BG Attributes and the OAM Attributes have bit 7 clear, the OBJ will have priority
        // Otherwise, BG will have priority.
        GbModel::Cgb => {
            if !lcd.control.is_bgw_enabled() {
                return true;
            }

            if bg_flags.is_bgw_priority() {
                return false;
            }

            if obj_flags.is_bgw_priority() {
                return false;
            }

            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bank 1 holds a different line than bank 0, so fetching the wrong one shows up
    /// as the wrong bytes rather than merely as a blank sprite.
    const BANK0_LINE: (u8, u8) = (0xF0, 0x0F);
    const BANK1_LINE: (u8, u8) = (0xAA, 0x55);
    /// Line 0 of tile 1, which is where the sprite below points.
    const TILE_ADDR: u16 = TILE_SET_DATA_1_START + TILE_BIT_SIZE;

    fn vram() -> VideoRam {
        let mut vram = VideoRam::default();

        for (bank, (byte0, byte1)) in [(0, BANK0_LINE), (1, BANK1_LINE)] {
            vram.write_bank_number(bank);
            vram.write(TILE_ADDR, byte0);
            vram.write(TILE_ADDR + 1, byte1);
        }

        vram
    }

    /// One sprite on line 0 at the left edge, with bit 3 of its flags set — the CGB
    /// "tile from bank 1" attribute, and junk on a DMG.
    fn fetcher() -> SpriteFetcher {
        let mut fetcher = SpriteFetcher::default();
        fetcher.line_sprites[0] = OamEntry {
            y: 16,
            x: 8,
            tile_index: 1,
            flags: TileFlags::from(0b0000_1000),
        };
        fetcher.line_sprites_count = 1;

        fetcher
    }

    fn fetched_line(model: GbModel, dmg_compat: bool) -> (u8, u8) {
        let mut lcd = Lcd::new([PixelColor::default(); 4], model);
        lcd.dmg_compat = dmg_compat;
        let mut fetcher = fetcher();
        fetcher.fetch(&lcd, &vram(), 0, 0);

        assert_eq!(
            fetcher.fetched_sprites_count, 1,
            "the sprite was not fetched"
        );
        let line = fetcher.fetched_sprites[0].tile_line;

        (line.byte0, line.byte1)
    }

    #[test]
    fn a_cgb_game_takes_the_bank_its_attribute_asks_for() {
        assert_eq!(fetched_line(GbModel::Cgb, false), BANK1_LINE);
    }

    /// The regression this guards: a DMG game leaves junk in the CGB attribute bits
    /// (ZAS sets bit 3 on every sprite), and reading bank 1 for it hands back the
    /// blank bank such a game never wrote — every sprite disappears.
    #[test]
    fn outside_cgb_mode_the_bank_attribute_is_ignored() {
        assert_eq!(fetched_line(GbModel::Dmg, false), BANK0_LINE);
        assert_eq!(fetched_line(GbModel::Cgb, true), BANK0_LINE);
    }
}

use crate::emu::config::GbModel;
use crate::ppu::fifo::PixelFifo;
use crate::ppu::lcd::Lcd;
use crate::ppu::sprites::SpriteFetcher;
use crate::ppu::tile::{TileFlags, TileLineData, TILE_BITS_COUNT, TILE_HEIGHT, TILE_WIDTH};
use crate::ppu::vram::{VideoRam, VRAM_ADDR_START};
use serde::{Deserialize, Serialize};

type FetchFn = fn(&mut PixelFetcher, &Lcd, &VideoRam);

const FETCH_FNS: [FetchFn; 4] = [
    PixelFetcher::fetch_tile,
    PixelFetcher::fetch_data0,
    PixelFetcher::fetch_data1,
    PixelFetcher::fetch_push,
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BgwFetchedData {
    pub tile_line: TileLineData,
    pub is_window: bool,
    pub cgb_flags: TileFlags,
    pub map_y: u8,
    pub tile_index: u8,
    /// Tilemap address the index was read from; the LCDC.4 mid-fetch glitch
    /// re-reads it as tile data.
    #[serde(default)]
    pub map_addr: u16,
    /// LCDC.4 (tile data area is $8000) latched when the tile index was
    /// fetched, to detect a mid-fetch flip at the data1 read.
    #[serde(default)]
    pub area_8000: bool,
}

#[inline(always)]
pub fn calc_bgw_tile_addr(tile_idx: u8, map_y: u8, data_area: u16) -> u16 {
    let tile_y = (map_y % TILE_HEIGHT as u8) * 2;

    data_area
        .wrapping_add(tile_idx as u16 * 16)
        .wrapping_add(tile_y as u16)
}

#[inline(always)]
pub fn bgw_tile_index_in_area(tile_idx: u8, data_area: u16) -> u8 {
    if data_area == 0x8800 {
        return tile_idx.wrapping_add(128);
    }

    tile_idx
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PixelFetcher {
    pub sprite_fetcher: SpriteFetcher,
    fetch_step: FetchStep,
    /// X position for a tile on the current line
    fetch_x: u8,
    pixel_fifo: PixelFifo,
    bgw_fetched_data: BgwFetchedData,
    /// Fine-scroll pixels to discard, latched from `SCX & 7` at the start of
    /// mode 3. Latched (not read live) so a mid-scanline SCX write can't change
    /// how many pixels are dropped at the left edge.
    scx_discard: u8,
    /// The window trigger fired on this line (screen x reached WX-7 with the
    /// window enabled): the BG fifo was dropped and fetching switched to
    /// window tiles. Stays set for the rest of the line even if the window is
    /// disabled mid-line, so a re-enable resumes instead of re-triggering.
    #[serde(default)]
    in_window: bool,
    /// Window-space tile counter, incremented per window fetch. Independent of
    /// `fetch_x` so the window always starts at its own column 0 regardless of
    /// where on the line it triggered.
    #[serde(default)]
    win_fetch_x: u8,
    /// Pixels to drop from the first window tile when WX < 7 (the window
    /// hangs off the left edge of the screen).
    #[serde(default)]
    win_discard: u8,
    /// Remaining sprite-fetch stall dots; while non-zero the fetcher and the
    /// pixel output are frozen (extends mode 3).
    #[serde(default)]
    stall_dots: u8,
    /// Dots the fetcher has run this line (excluding stalls). Drives the
    /// 2-dots-per-step cadence so a stall of odd length can't desync it.
    #[serde(default)]
    fetch_dot: u16,
}

impl Default for PixelFetcher {
    fn default() -> PixelFetcher {
        Self {
            fetch_step: FetchStep::Tile,
            pixel_fifo: Default::default(),
            fetch_x: 0,
            bgw_fetched_data: Default::default(),
            sprite_fetcher: Default::default(),
            scx_discard: 0,
            in_window: false,
            win_fetch_x: 0,
            win_discard: 0,
            stall_dots: 0,
            fetch_dot: 0,
        }
    }
}

impl PixelFetcher {
    #[inline(always)]
    pub fn tick(&mut self, lcd: &mut Lcd, vram: &VideoRam, _line_ticks: usize) {
        // Window trigger: the dot where the next screen pixel is WX-7 (or 0
        // when WX < 7, with the off-screen part of the first tile dropped).
        // In-flight BG pixels are discarded and fetching restarts at the
        // window's own column 0 — pixel-exact, independent of the tile grid.
        if !self.in_window && lcd.window.on(lcd) {
            let wx = lcd.window.x;
            let trigger_x = wx.saturating_sub(7);

            if lcd.buffer.count_x() == trigger_x as usize {
                self.in_window = true;
                self.win_discard = 7u8.saturating_sub(wx);
                // Drop the in-flight BG pixels but keep the line's stream
                // position, and rewind the fetch column to match — sprite
                // mixing and a later mid-line window-off both stay aligned.
                self.pixel_fifo.restart();
                self.fetch_x = self.pixel_fifo.pushed_count();
                self.fetch_step = FetchStep::Tile;
            }
        }

        // Sprite fetches stall the whole pipeline, extending mode 3.
        if self.stall_dots > 0 {
            self.stall_dots -= 1;
            return;
        }

        if lcd.control.is_obj_enabled() {
            let x = lcd.buffer.count_x() as u8;
            let stall = self.sprite_fetcher.take_penalty(x, lcd.scroll_x);

            if stall > 0 {
                self.stall_dots = stall - 1; // this dot is part of the stall
                return;
            }
        }

        self.fetch_dot += 1;

        // The first three steps take 2 dots each and the push step is attempted every dot until it succeeds
        if self.fetch_dot & 1 != 0 || self.fetch_step == FetchStep::Push {
            // SAFETY: we control FETCH_FNS and FetchStep
            unsafe {
                FETCH_FNS.get_unchecked(self.fetch_step as usize)(self, lcd, vram);
            }
        }

        // pop fifo to lcd
        if let Some((pixel, x)) = self.pixel_fifo.pop() {
            if self.in_window {
                // Window pixels bypass the SCX fine scroll; when WX < 7 the
                // first tile hangs off the left edge and its hidden pixels
                // are dropped instead.
                if self.win_discard > 0 {
                    self.win_discard -= 1;
                } else {
                    lcd.push_pixel(pixel);
                }
            } else if x >= self.scx_discard {
                // Drop the first `SCX & 7` background pixels (fine horizontal
                // scroll), latched at mode-3 start.
                lcd.push_pixel(pixel);
            };
        }
    }

    /// Pushes the fetched tile row (8 pixels) unless the FIFO still holds the
    /// previous one; returns whether it pushed.
    #[inline(always)]
    fn fifo_push(&mut self, lcd: &Lcd) -> bool {
        if self.pixel_fifo.is_full() {
            return false;
        }

        let bg_enabled = lcd.control.is_bgw_enabled();
        let bg_flags = self.bgw_fetched_data.cgb_flags;
        // X-flip reverses the bitplanes once; the loop then walks MSB-first
        // with constant-distance shifts instead of re-indexing per pixel.
        let (mut byte0, mut byte1) = if bg_flags.is_x_flip() {
            (
                self.bgw_fetched_data.tile_line.byte0.reverse_bits(),
                self.bgw_fetched_data.tile_line.byte1.reverse_bits(),
            )
        } else {
            (
                self.bgw_fetched_data.tile_line.byte0,
                self.bgw_fetched_data.tile_line.byte1,
            )
        };
        let mut fifo_x = self.pixel_fifo.pushed_count();

        for _ in 0..TILE_BITS_COUNT {
            let bg_color_id = ((byte1 >> 6) & 0b10 | byte0 >> 7) as usize;
            byte0 <<= 1;
            byte1 <<= 1;

            let sp_color = self
                .sprite_fetcher
                .get_color(lcd, fifo_x, bg_color_id, bg_flags);

            if let Some(sp_color) = sp_color {
                self.pixel_fifo.push(sp_color);
            } else {
                let bgw_color = lcd.get_bgw_color(bg_color_id, bg_enabled, bg_flags);
                self.pixel_fifo.push(bgw_color);
            }

            fifo_x += 1;
        }

        true
    }

    #[inline(always)]
    fn fetch_tile(&mut self, lcd: &Lcd, vram: &VideoRam) {
        let lcdc = lcd.control;

        // In CGB when LCDC bit 0 = 0, BG and Window are still drawn
        // But OBJ always has priority over BG,
        if lcdc.is_bgw_enabled() || lcd.model == GbModel::Cgb {
            // Once triggered, the window supplies tiles until the end of the
            // line unless it gets disabled mid-line; its row comes from the
            // internal line counter, its column from the window-space fetch
            // counter (both independent of LY/SCX).
            let (map_y, tilemap_addr) = if self.in_window && lcd.window.is_visible(lcd) {
                self.bgw_fetched_data.is_window = true;
                let map_y = lcd.window.line_number;
                let tilemap_addr = lcd.control.get_win_map_area()
                    + (self.win_fetch_x as u16 & 31)
                    + ((map_y as u16 / TILE_HEIGHT) * 32);
                self.win_fetch_x = self.win_fetch_x.wrapping_add(1);

                (map_y, tilemap_addr)
            } else {
                let map_y = lcd.ly.wrapping_add(lcd.scroll_y);
                let map_x = self.fetch_x.wrapping_add(lcd.scroll_x);
                let tilemap_addr = lcdc.get_bg_map_area()
                    + (map_x as u16 / TILE_WIDTH)
                    + ((map_y as u16 / TILE_HEIGHT) * 32);
                self.bgw_fetched_data.is_window = false;

                (map_y, tilemap_addr)
            };

            let cgb_flags = vram.read_tile_flags(tilemap_addr);
            let y_flip = cgb_flags.is_y_flip();
            let row_in_tile = map_y & 7;
            let row = if y_flip { 7 - row_in_tile } else { row_in_tile };
            // Replace map_y's low 3 bits with flipped row
            self.bgw_fetched_data.map_y = (map_y & !7) | row;
            self.bgw_fetched_data.cgb_flags = cgb_flags;
            // store only tile index because data area could change between data reads so read it on data steps
            // tile map is olways in bank 0
            self.bgw_fetched_data.tile_index = vram.read_from_bank(0, tilemap_addr);
            self.bgw_fetched_data.map_addr = tilemap_addr;
            self.bgw_fetched_data.area_8000 = lcdc.get_bgw_data_area() == 0x8000;
        } else {
            // BGW off on DMG: no tile is fetched, but fetch_step still reaches
            // data1, whose LCDC.4 mid-fetch glitch may re-read map_addr. Keep it
            // a valid VRAM address so the read can't underflow; the pixel is
            // forced blank by get_bgw_color, so the value read is discarded.
            self.bgw_fetched_data.map_addr = VRAM_ADDR_START;
            self.bgw_fetched_data.area_8000 = lcdc.get_bgw_data_area() == 0x8000;
        }

        if lcdc.is_obj_enabled() {
            self.sprite_fetcher
                .fetch(lcd, vram, lcd.scroll_x, self.fetch_x);
        }

        self.fetch_step = FetchStep::Data0;
        self.fetch_x = self.fetch_x.wrapping_add(TILE_WIDTH as u8);
    }

    #[inline(always)]
    fn read_tile_byte(&mut self, lcd: &Lcd, vram: &VideoRam, byte: usize) -> u8 {
        let data_area = lcd.control.get_bgw_data_area();
        let tile_index = bgw_tile_index_in_area(self.bgw_fetched_data.tile_index, data_area);
        let tiledata_addr = calc_bgw_tile_addr(tile_index, self.bgw_fetched_data.map_y, data_area);
        let vram_bank = self.bgw_fetched_data.cgb_flags.read_cgb_vram_bank();

        vram.read_tile_byte_from_bank(vram_bank, tiledata_addr, byte)
    }

    #[inline(always)]
    fn fetch_data0(&mut self, lcd: &Lcd, vram: &VideoRam) {
        self.bgw_fetched_data.tile_line.byte0 = self.read_tile_byte(lcd, vram, 0);
        self.fetch_step = FetchStep::Data1;
    }

    #[inline(always)]
    fn fetch_data1(&mut self, lcd: &Lcd, vram: &VideoRam) {
        // LCDC.4 mid-fetch glitch: flipping the tile data area from $8800 to
        // $8000 mid-fetch makes the second data read go through the tilemap
        // address instead, returning the tile index byte (cgb-acid-hell draws
        // its face with this).
        let area_8000 = lcd.control.get_bgw_data_area() == 0x8000;

        self.bgw_fetched_data.tile_line.byte1 = if area_8000 && !self.bgw_fetched_data.area_8000 {
            vram.read_from_bank(0, self.bgw_fetched_data.map_addr)
        } else {
            self.read_tile_byte(lcd, vram, 1)
        };
        self.fetch_step = FetchStep::Push;
    }

    #[inline(always)]
    fn fetch_push(&mut self, lcd: &Lcd, _: &VideoRam) {
        if self.fifo_push(lcd) {
            self.fetch_step = FetchStep::Tile;
        }
    }

    #[inline(always)]
    pub const fn reset(&mut self, scroll_x: u8) {
        self.fetch_step = FetchStep::Tile;
        self.pixel_fifo.clear();
        self.fetch_x = 0;
        self.scx_discard = scroll_x % TILE_WIDTH as u8;
        self.in_window = false;
        self.win_fetch_x = 0;
        self.win_discard = 0;
        self.stall_dots = 0;
        self.fetch_dot = 0;
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FetchStep {
    Tile,
    Data0,
    Data1,
    Push,
}

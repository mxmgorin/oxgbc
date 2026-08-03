use core::ppu::tile::{
    TileData, TILES_COUNT, TILE_BITS_COUNT, TILE_HEIGHT, TILE_LINE_BYTES_COUNT, TILE_WIDTH,
};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;
use sdl2::VideoSubsystem;

const SCALE: u32 = 5;
const SPACER: i32 = (8 * SCALE) as i32;
const TILE_ROWS: i32 = 24;
const TILE_COLS: i32 = 16;
const Y_SPACER: i32 = SCALE as i32;
const X_DRAW_START: i32 = (SCALE / 2) as i32;

pub struct Sdl2TilesView {
    canvas: Canvas<Window>,
    rect_groups: [Vec<Rect>; 4],
    tiles: [TileData; TILES_COUNT],
}

impl Sdl2TilesView {
    pub fn new(video_subsystem: &VideoSubsystem) -> Sdl2TilesView {
        let tile_grid_width =
            TILE_COLS as u32 * TILE_WIDTH as u32 * SCALE + (TILE_COLS as u32 * SCALE);
        let tile_grid_height = TILE_ROWS as u32 * TILE_HEIGHT as u32 * SCALE + 122;

        let debug_window = video_subsystem
            .window("Tiles", tile_grid_width, tile_grid_height)
            .position_centered()
            .build()
            .unwrap();

        Self {
            canvas: debug_window.into_canvas().build().unwrap(),
            rect_groups: allocate_rect_groups(
                TILES_COUNT * TILE_LINE_BYTES_COUNT * TILE_BITS_COUNT as usize * 4,
            ),
            tiles: [TileData::default(); TILES_COUNT],
        }
    }

    pub fn window_id(&self) -> u32 {
        self.canvas.window().id()
    }

    pub fn draw_tiles(&mut self, tiles: impl Iterator<Item = TileData>) {
        for (dst, src) in self.tiles.iter_mut().zip(tiles) {
            *dst = src;
        }

        let mut col_x_draw = X_DRAW_START;
        let mut row_y_draw: i32 = 0;
        let mut tile_num = 0;
        let mut rect_counts: [usize; 4] = [0; 4];
        self.canvas.set_draw_color(Color::RGB(18, 18, 18));
        self.canvas.fill_rect(None).unwrap();

        for row in 0..TILE_ROWS {
            for col in 0..TILE_COLS {
                let tile = self.tiles[tile_num as usize];

                for (line_y, line) in tile.lines.iter().enumerate() {
                    for (color_x, color_id) in line.iter_color_ids().enumerate() {
                        let color_index = color_id as usize;
                        let rect = &mut self.rect_groups[color_index][rect_counts[color_index]];
                        rect.x =
                            col_x_draw + (col * SCALE as i32) + (color_x as i32 * SCALE as i32);
                        rect.y = row_y_draw + (row + SCALE as i32) + (line_y as i32 * SCALE as i32);
                        rect_counts[color_index] += 1;
                    }
                }

                col_x_draw += SPACER;
                tile_num += 1;
            }

            row_y_draw += SPACER + Y_SPACER;
            col_x_draw = X_DRAW_START;
        }

        fill_rect_groups(&mut self.canvas, &self.rect_groups, rect_counts);
        self.canvas.present();
    }
}

fn allocate_rect_groups(len: usize) -> [Vec<Rect>; 4] {
    let mut recs = Vec::with_capacity(len);

    for _ in 0..recs.capacity() {
        recs.push(Rect::new(0, 0, SCALE, SCALE));
    }

    [recs.clone(), recs.clone(), recs.clone(), recs]
}

fn fill_rect_groups(canvas: &mut Canvas<Window>, item_groups: &[Vec<Rect>; 4], count: [usize; 4]) {
    for (color_id, items) in item_groups.iter().enumerate() {
        canvas.set_draw_color(SDL_COLORS[color_id]);
        canvas.fill_rects(&items[..count[color_id]]).unwrap();
    }
}

const SDL_COLORS: [Color; 4] = [
    Color::WHITE,
    Color::RGB(170, 170, 170), // Light Gray
    Color::RGB(85, 85, 85),    // Dark Gray
    Color::BLACK,
];

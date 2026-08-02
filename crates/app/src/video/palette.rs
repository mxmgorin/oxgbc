use crate::storage::get_base_dir;
use core::save_json_file;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LcdPalette {
    pub name: String,
    pub hex_colors: [String; 4],
}

impl LcdPalette {
    /// The stored palettes, or the built-in set written to disk for editing.
    pub fn load_or_create() -> Box<[LcdPalette]> {
        let path = Self::default_palettes_path();

        if path.exists() {
            return core::read_json_file(&path).unwrap();
        }

        let palettes = Self::default_palettes().into_boxed_slice();
        Self::save_palettes_file(&palettes).unwrap();

        palettes
    }

    pub fn save_palettes_file(palettes: &Box<[LcdPalette]>) -> Result<(), io::Error> {
        let path = Self::default_palettes_path();

        save_json_file(&path, palettes)
    }

    pub fn default_palettes_path() -> PathBuf {
        get_base_dir().join("palettes.json")
    }

    pub fn default_palettes() -> Vec<LcdPalette> {
        vec![
            LcdPalette::forgotten_swamp(),
            LcdPalette::crimson(),
            LcdPalette::nostalgia(),
            LcdPalette::bit2_matrix(),
            LcdPalette::purple_dawn(),
            LcdPalette::cave_4(),
            LcdPalette::gb_mcebuius(),
            LcdPalette::candy_pop(),
            LcdPalette::red_is_dead(),
            LcdPalette::rabbit_5pm(),
            LcdPalette::blueberry(),
            LcdPalette::peach_blizzard(),
            LcdPalette::m_gb(),
            LcdPalette {
                name: "Swamp".to_string(),
                hex_colors: [
                    "fafca4ff".to_string(),
                    "d68e49ff".to_string(),
                    "308013ff".to_string(),
                    "400c01ff".to_string(),
                ],
            },
            LcdPalette {
                name: "2bit demichrome".to_string(),
                hex_colors: [
                    "211e20ff".to_string(),
                    "555568ff".to_string(),
                    "a0a08bff".to_string(),
                    "e9efecff".to_string(),
                ],
            },
            LcdPalette {
                name: "SpaceHaze".to_string(),
                hex_colors: [
                    "f8e3c4ff".to_string(),
                    "cc3495ff".to_string(),
                    "6b1fb1ff".to_string(),
                    "0b0630ff".to_string(),
                ],
            },
        ]
    }

    pub fn nostalgia() -> Self {
        Self {
            name: "Nostalgia".to_string(),
            hex_colors: [
                "d0d058ff".to_string(),
                "a0a840ff".to_string(),
                "708028ff".to_string(),
                "405010ff".to_string(),
            ],
        }
    }

    pub fn bit2_matrix() -> Self {
        Self {
            name: "2 bit matrix".to_string(),
            hex_colors: [
                "f2fff2ff".to_string(),
                "add9bcff".to_string(),
                "5b8c7cff".to_string(),
                "0d1a1aff".to_string(),
            ],
        }
    }

    // Created by WildLeoKnight
    pub fn purple_dawn() -> Self {
        Self {
            name: "Purple dawn".to_string(),
            hex_colors: [
                "eefdedff".to_string(),
                "9a7bbcff".to_string(),
                "2d757eff".to_string(),
                "001b2eff".to_string(),
            ],
        }
    }

    // Created by Qirlfriend
    pub fn peach_blizzard() -> Self {
        Self {
            name: "Peach blizzard".to_string(),
            hex_colors: [
                "fbd6adff".to_string(),
                "f19386ff".to_string(),
                "a54371ff".to_string(),
                "4c388aff".to_string(),
            ],
        }
    }

    // Created by Isa
    pub fn blueberry() -> Self {
        Self {
            name: "Blueberry".to_string(),
            hex_colors: [
                "280b0bff".to_string(),
                "6c2e53ff".to_string(),
                "aa73e0ff".to_string(),
                "b0ecf9ff".to_string(),
            ],
        }
    }

    // Created by Retronika
    pub fn m_gb() -> Self {
        Self {
            name: "M-GB".to_string(),
            hex_colors: [
                "dbd3c9ff".to_string(),
                "a3bfa2ff".to_string(),
                "788f97ff".to_string(),
                "546a76ff".to_string(),
            ],
        }
    }

    // Created by RABBITKNG
    pub fn rabbit_5pm() -> Self {
        Self {
            name: "Rabbit 5pm".to_string(),
            hex_colors: [
                "ffe7cdff".to_string(),
                "e4a39fff".to_string(),
                "629098ff".to_string(),
                "4c3457ff".to_string(),
            ],
        }
    }

    pub fn candy_pop() -> Self {
        Self {
            name: "Candy pop!".to_string(),
            hex_colors: [
                "301221ff".to_string(),
                "854576ff".to_string(),
                "9e81d0ff".to_string(),
                "eebff5ff".to_string(),
            ],
        }
    }

    // Created by Devine Devine
    pub fn red_is_dead() -> Self {
        Self {
            name: "Red is dead".to_string(),
            hex_colors: [
                "fffcfeff".to_string(),
                "ff0015ff".to_string(),
                "860020ff".to_string(),
                "11070aff".to_string(),
            ],
        }
    }

    // Created by Polyducks
    pub fn cave_4() -> Self {
        Self {
            name: "Cave4".to_string(),
            hex_colors: [
                "e4cbbfff".to_string(),
                "938282ff".to_string(),
                "4f4e80ff".to_string(),
                "2c0016ff".to_string(),
            ],
        }
    }

    // Created by RABBITKNG
    pub fn gb_mcebuius() -> Self {
        Self {
            name: "Mcebuius".to_string(),
            hex_colors: [
                "f1e0cdff".to_string(),
                "ffa49aff".to_string(),
                "da3467ff".to_string(),
                "35333fff".to_string(),
            ],
        }
    }

    pub fn black_white() -> Self {
        Self {
            name: "Back and White".to_string(),
            hex_colors: [
                "ffffffff".to_string(),
                "AAAAAAff".to_string(),
                "555555ff".to_string(),
                "000000ff".to_string(),
            ],
        }
    }

    pub fn forgotten_swamp() -> Self {
        Self {
            name: "Forgotten Swamp".to_string(),
            hex_colors: [
                "d1ada1ff".to_string(),
                "4d7d65ff".to_string(),
                "593a5fff".to_string(),
                "3b252eff".to_string(),
            ],
        }
    }

    pub fn crimson() -> Self {
        Self {
            name: "Crimson".to_string(),
            hex_colors: [
                "eff9d6ff".to_string(),
                "ba5044ff".to_string(),
                "7a1c4bff".to_string(),
                "1b0326ff".to_string(),
            ],
        }
    }

    pub fn rustic() -> Self {
        LcdPalette {
            name: "Rustic".to_string(),
            hex_colors: [
                "2c2137ff".to_string(),
                "764462ff".to_string(),
                "a96868ff".to_string(),
                "edb4a1ff".to_string(),
            ],
        }
    }
}

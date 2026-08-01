//! What the app knows about a ROM besides its bytes: the name the user gave it,
//! and the facts read out of its header.
//!
//! Kept in `library/` under the app's own directory rather than beside the ROM: a
//! collection often sits on a read-only or shared disk, and nothing of ours
//! belongs in it. Keyed by file name, like every other save.

use crate::get_base_dir;
use crate::state_meta::CartId;
use core::cart::header::{CartHeader, CgbFlag};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LIBRARY_DIR: &str = "library";
const META_EXT: &str = "json";

/// Every field defaults, so a sidecar written by an older build still loads and
/// new fields can be added without a migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RomMeta {
    /// What the user called it; empty leaves the file name standing in.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cart: CartId,
    #[serde(default)]
    pub cgb: CgbFlag,
    /// Length of the ROM the facts above were read from. A file replaced by one of
    /// another size gets read again.
    #[serde(default)]
    pub rom_len: u64,
}

impl RomMeta {
    /// The sidecar for `name`, reading the ROM's header only when there is no
    /// sidecar yet or the ROM behind it changed. Browsing the library never has to
    /// open the ROM files themselves — they are the big ones, and often the ones on
    /// a slow disk.
    pub fn load_or_create(path: &Path, name: &str) -> Self {
        let rom_len = fs::metadata(path)
            .map(|meta| meta.len())
            .unwrap_or_default();
        let stored = Self::load_file(name).ok();

        if let Some(meta) = stored.as_ref().filter(|meta| meta.rom_len == rom_len) {
            return meta.clone();
        }

        // Whatever the user called it survives the ROM changing under it.
        let mut meta = stored.unwrap_or_default();

        let Some(header) = read_header(path) else {
            return meta;
        };

        meta.cart = CartId::of_header(&header);
        meta.cgb = CartHeader::parse_cgb_flag(&header);
        meta.rom_len = rom_len;

        if let Err(err) = meta.save_file(name) {
            log::warn!("Failed save rom meta: {err}");
        }

        meta
    }

    pub fn save_file(&self, name: &str) -> Result<(), String> {
        let path = Self::path(name);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn load_file(name: &str) -> Result<Self, String> {
        let data = fs::read_to_string(Self::path(name)).map_err(|e| e.to_string())?;

        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    pub fn path(name: &str) -> PathBuf {
        get_base_dir()
            .join(LIBRARY_DIR)
            .join(format!("{name}.{META_EXT}"))
    }
}

fn read_header(path: &Path) -> Option<[u8; CartHeader::END]> {
    let mut header = [0; CartHeader::END];
    File::open(path).ok()?.read_exact(&mut header).ok()?;

    Some(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_goes_by_the_rom_file_name() {
        let path = RomMeta::path("Zelda.gbc");

        assert_eq!(path.file_name().unwrap(), "Zelda.gbc.json");
        assert_eq!(path.parent().unwrap().file_name().unwrap(), LIBRARY_DIR);
    }

    /// Fields were added over time, so old sidecars must still parse.
    #[test]
    fn an_older_sidecar_loads_with_defaults() {
        let meta: RomMeta = serde_json::from_str(r#"{"name":"Ninja Gaiden Shadow"}"#).unwrap();

        assert_eq!(meta.name, "Ninja Gaiden Shadow");
        assert_eq!(meta.cgb, CgbFlag::DmgOnly);
        assert_eq!(meta.rom_len, 0);
        assert_eq!(meta.cart, CartId::default());
    }
}

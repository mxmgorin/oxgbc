//! Zipped ROMs, which is how a collection is often shipped. Only the first `.gb`
//! or `.gbc` in the archive is taken; whatever else it holds is not ours to guess
//! about.

use std::io::{Cursor, Read, Seek};
use std::path::Path;
use zip::ZipArchive;

pub fn is_zip(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    extension == "zip"
}

/// The whole cartridge, to run it.
pub fn unzip_rom(bytes: &[u8]) -> Result<Vec<u8>, String> {
    read_rom(Cursor::new(bytes), None)
}

/// Its first `len` bytes, inflating no further. Cataloguing a shelf of zips wants
/// headers: unpacking each cartridge whole to reach one cost a quarter of a second
/// per game on a handheld's card.
pub fn unzip_rom_prefix<R: Read + Seek>(reader: R, len: usize) -> Result<Vec<u8>, String> {
    read_rom(reader, Some(len))
}

fn read_rom<R: Read + Seek>(reader: R, len: Option<usize>) -> Result<Vec<u8>, String> {
    let mut archive = ZipArchive::new(reader).map_err(|_| "Invalid zip archive".to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|err| err.to_string())?;

        if !is_rom_entry(file.name()) {
            continue;
        }

        let mut buffer = Vec::new();
        match len {
            Some(len) => {
                buffer.resize(len, 0);
                file.read_exact(&mut buffer)
                    .map_err(|err| err.to_string())?;
            }
            None => {
                file.read_to_end(&mut buffer)
                    .map_err(|err| err.to_string())?;
            }
        }

        return Ok(buffer);
    }

    Err("No valid .gb or .gbc file found in zip".to_string())
}

fn is_rom_entry(name: &str) -> bool {
    let name = name.to_ascii_lowercase();

    name.ends_with(".gb") || name.ends_with(".gbc")
}

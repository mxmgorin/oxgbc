//! Zipped ROMs, which is how a collection is often shipped. Only the first `.gb`
//! or `.gbc` in the archive is taken; whatever else it holds is not ours to guess
//! about.

use std::io::{Cursor, Read};
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

pub fn unzip_rom(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|_| "Invalid zip archive".to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().to_ascii_lowercase();

        if name.ends_with(".gb") || name.ends_with(".gbc") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            return Ok(buffer);
        }
    }

    Err("No valid .gb or .gbc file found in zip".to_string())
}

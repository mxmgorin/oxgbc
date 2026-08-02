//! Everything an app writes beside a game: its save states with their sidecars,
//! and the cartridge's own battery-backed RAM.
//!
//! A state is three files derived from one path — the state itself, its metadata
//! and its screen — so they cannot drift apart.

pub mod battery;
pub mod meta;
pub mod shot;

use crate::storage::get_base_dir;
use core::emu::state::EmuSaveState;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn write_state(state: &EmuSaveState, name: &str, suffix: &str) -> Result<(), String> {
    let path = state_path(name, suffix);

    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut file = File::create(path).map_err(|e| e.to_string())?;
    postcard::to_io(state, &mut file).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn read_state(name: &str, suffix: &str) -> Result<EmuSaveState, String> {
    let path = state_path(name, suffix);
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let decoded: EmuSaveState = postcard::from_bytes(&buffer).map_err(|e| e.to_string())?;

    Ok(decoded)
}

pub fn delete_state(name: &str, suffix: &str) -> Result<(), String> {
    let path = state_path(name, suffix);

    fs::remove_file(path).map_err(|e| e.to_string())
}

/// The state file itself; [`meta`] and [`shot`] derive their own paths from this
/// one, so a slot's three files always sit together.
pub fn state_path(game_name: &str, suffix: &str) -> PathBuf {
    get_base_dir()
        .join("save_states")
        .join(format!("{game_name}_{suffix}.state"))
}

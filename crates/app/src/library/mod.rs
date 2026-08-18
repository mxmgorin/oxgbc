//! The game collection: which ROMs the app knows about, and what it keeps beside
//! each one — the user's name for it, its header facts, its cover.

pub mod cover;
pub mod meta;

use crate::storage::base_dir;
use crate::PlatformFileSystem;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

/// The folder a fresh install shelves and browses first: only a launcher knows
/// where a device keeps its ROMs.
const ROMS_DIR_ENV: &str = "OXGBC_ROMS_DIR";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RomsState {
    pub last_browse_dir_path: Option<PathBuf>,
    pub selected_dir_path: Option<PathBuf>,
    opened_rom_paths: IndexSet<PathBuf>,
    loaded_rom_files: HashSet<String>,
    /// Wall-clock seconds played, by file name rather than path — the same key
    /// battery saves and states go by, so moving a ROM keeps its history.
    #[serde(default)]
    playtime_secs: HashMap<String, u64>,
}

impl RomsState {
    pub fn add_playtime(&mut self, game: &str, secs: u64) {
        *self.playtime_secs.entry(game.to_owned()).or_default() += secs;
    }

    pub fn playtime(&self, game: &str) -> u64 {
        self.playtime_secs.get(game).copied().unwrap_or_default()
    }

    pub fn opened_count(&self) -> usize {
        self.opened_rom_paths.len()
    }

    pub fn loaded_count(&self) -> usize {
        self.loaded_rom_files.len()
    }

    /// Loads all `.gb` and `.gbc` files from the given directory.
    pub fn load_from_dir<P: AsRef<Path>>(
        &mut self,
        dir: P,
        filesystem: &impl PlatformFileSystem,
    ) -> Result<usize, String> {
        let dir_path = dir.as_ref();
        self.loaded_rom_files.clear();
        let files = filesystem.read_dir(dir_path)?;
        self.selected_dir_path = Some(dir_path.to_owned());
        let can_split_paths = filesystem.can_split_paths();

        for file in files {
            if file.ends_with(".gb") || file.ends_with(".gbc") {
                if can_split_paths {
                    let path = PathBuf::from(file);
                    if let Some(name) = filesystem.file_name(&path) {
                        self.loaded_rom_files.insert(name); // store just the name
                    }
                } else {
                    self.loaded_rom_files.insert(file.clone()); // store full path
                }
            }
        }

        Ok(self.loaded_rom_files.len())
    }

    /// Stored absolute where the platform has real paths: a game launched as
    /// `roms/game.gb` would otherwise point nowhere the next time the app starts
    /// somewhere else, and would sit beside its own absolute self on the shelf.
    /// What cannot be canonicalized — Android hands out `content://` URIs — is kept
    /// as it came.
    pub fn insert_or_update(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);

        self.opened_rom_paths.shift_remove(&path);
        self.opened_rom_paths.insert(path);
    }

    pub fn remove(&mut self, path: &Path) {
        self.opened_rom_paths.shift_remove(path);
    }

    pub fn last_path(&self) -> Option<&PathBuf> {
        self.opened_rom_paths.iter().last()
    }

    pub fn load_or_create(fs: &impl PlatformFileSystem) -> Self {
        let path = Self::path();

        let mut obj = if path.exists() {
            core::read_json_file(&path).unwrap_or_else(|_| Self::seeded())
        } else {
            Self::seeded()
        };

        // Paths written before they were stored absolute, and any that have since
        // been reached another way, settle here — otherwise a game keeps a second
        // spelling of itself for good.
        obj.opened_rom_paths = obj
            .opened_rom_paths
            .drain(..)
            .map(|path| path.canonicalize().unwrap_or(path))
            .collect();

        if let Some(path) = obj.selected_dir_path.take() {
            if let Err(err) = obj.load_from_dir(path, fs) {
                log::error!("Failed load_from_dir: {err}");
            }
        }

        obj
    }

    /// A library with nothing on disk behind it: whatever [`ROMS_DIR_ENV`] names, or
    /// empty. A starting point only — the app keeps what the user picks next.
    fn seeded() -> Self {
        let Some(dir) = env::var_os(ROMS_DIR_ENV).filter(|dir| !dir.is_empty()) else {
            return Default::default();
        };
        let dir = PathBuf::from(dir);

        Self {
            last_browse_dir_path: Some(dir.clone()),
            selected_dir_path: Some(dir),
            ..Default::default()
        }
    }

    /// Returns an iterator over the full paths of loaded ROM files.
    pub fn iter_loaded(
        &self,
        fs: &impl PlatformFileSystem,
    ) -> Option<impl Iterator<Item = PathBuf> + '_> {
        let can_split_paths = fs.can_split_paths();

        self.selected_dir_path.as_ref().map(|dir| {
            self.loaded_rom_files.iter().map(move |file_name| {
                if can_split_paths {
                    dir.join(file_name)
                } else {
                    PathBuf::from(file_name)
                }
            })
        })
    }

    pub fn iter_opened(&self) -> impl Iterator<Item = &PathBuf> + '_ {
        self.opened_rom_paths.iter().rev()
    }

    pub fn save_file(&self) {
        if let Err(err) = core::save_json_file(RomsState::path(), self) {
            log::error!("Failed to save ROMs: {err}");
        }
    }

    fn path() -> PathBuf {
        base_dir().join("roms.json")
    }
}

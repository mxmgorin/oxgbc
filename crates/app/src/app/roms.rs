//! What the app does to a shelved game short of playing it: naming it, giving
//! it a cover, and choosing which folder the shelf lists.

use crate::app::App;
use crate::frontend::Frontend;
use crate::library::cover;
use crate::library::meta::RomMeta;
use crate::save::shot;
use crate::{PlatformFileDialog, PlatformFileSystem};
use std::path::Path;

impl<FS, FD> App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    /// Touches only the library sidecar — the ROM file itself is never written to.
    pub fn handle_rename_rom(&mut self, path: &Path, name: String) {
        let Some(game) = self.platform.fs.file_name(path) else {
            log::error!("Failed rename_rom: filesystem.file_name: None");
            return;
        };

        let mut meta = RomMeta::load_or_create(path, &game);
        meta.name = name;

        if let Err(err) = meta.save_file(&game) {
            log::error!("Failed rename_rom: {err}");
            return;
        }

        self.frontend.request_update();
        let msg = if meta.name.is_empty() {
            format!("Cleared name of {game}")
        } else {
            format!("Renamed to {}", meta.name)
        };
        self.notifications.add(msg);
    }

    /// A state's screen is already a PNG of the same kind a cover is, and smaller
    /// than the size covers are held to, so taking that file neither converts nor
    /// resizes anything. A state older than those files still has its screen inside
    /// it, at the price of decoding the whole state.
    pub fn handle_cover_from_state(&mut self, rom: &Path, index: usize) {
        let Some(game) = self.platform.fs.file_name(rom) else {
            log::error!("Failed cover_from_state: filesystem.file_name: None");
            return;
        };
        let suffix = index.to_string();
        let set = cover::import(&game, &shot::path(&game, &suffix)).or_else(|_| {
            let shot = shot::load_from_state(&game, &suffix)?;

            cover::set(&game, &shot)
        });

        if let Err(err) = set {
            log::error!("Failed cover_from_state: {err}");
            self.notifications.add("Failed to read that state");
            return;
        }

        self.frontend.request_update();
        self.notifications
            .add(format!("Cover set from state {index}"));
    }

    /// Shelves what is in the folder, whichever way it was chosen.
    pub fn use_roms_dir(&mut self, dir: &Path) {
        let result = self.roms.load_from_dir(dir, &self.platform.fs);

        let Ok(count) = result else {
            log::error!("Failed to load ROMs: {}", result.unwrap_err());
            return;
        };

        // The shelf lists this directory now, so it has to be rebuilt.
        self.frontend.request_update();
        self.notifications.add(format!("Found {count} ROMs"));
    }

    pub fn handle_remove_rom_cover(&mut self, rom: &Path) {
        let Some(game) = self.platform.fs.file_name(rom) else {
            log::error!("Failed remove_rom_cover: filesystem.file_name: None");
            return;
        };

        if let Err(err) = cover::delete(&game) {
            log::error!("Failed remove_rom_cover: {err}");
            return;
        }

        self.frontend.request_update();
        self.notifications.add(format!("Cover removed from {game}"));
    }

    /// Asks through the platform's own dialog. Devices without one walk storage in
    /// the app instead, and come back with [`Self::use_rom_cover`].
    pub fn ask_rom_cover(&mut self, rom: &Path) {
        let picked = self.platform.fd.select_file(
            "Select cover image",
            (&["*.png", "*.jpg", "*.jpeg"], "Images (*.png, *.jpg)"),
        );

        if let Some(picked) = picked {
            self.use_rom_cover(rom, Path::new(&picked));
        }
    }

    pub fn use_rom_cover(&mut self, rom: &Path, image: &Path) {
        let Some(game) = self.platform.fs.file_name(rom) else {
            log::error!("Failed use_rom_cover: filesystem.file_name: None");
            return;
        };

        if let Err(err) = cover::import(&game, image) {
            log::error!("Failed use_rom_cover: {err}");
            self.notifications.add("Failed to read that image");
            return;
        }

        self.frontend.request_update();
        self.notifications.add(format!("Cover set for {game}"));
    }
}

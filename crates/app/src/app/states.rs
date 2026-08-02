//! The save-state commands. A slot is a state file named after its index, plus
//! the sidecars that carry its name and its screen — all three move together.

use crate::app::{App, AppState};
use crate::config::RenderConfig;
use crate::frontend::Frontend;
use crate::library::meta::CartId;
use crate::save::meta::StateMeta;
use crate::save::{self, shot};
use crate::{PlatformFileDialog, PlatformFileSystem};
use core::emu::state::SaveStateCmd;
use core::emu::Emu;

impl<FS, FD> App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fn handle_save_state(&mut self, emu: &mut Emu, event: SaveStateCmd, index: Option<usize>) {
        let Some(name) = self.game_name() else {
            log::error!("Failed save_state: no game loaded");
            return;
        };

        match event {
            SaveStateCmd::Create => {
                let save_state = emu.create_save_state();
                let index = index.unwrap_or(self.config.current_save_slot).to_string();

                if let Err(err) = save::write_state(&save_state, &name, &index) {
                    log::error!("Failed save_state: {err}");
                    return;
                }

                // Overwriting a slot keeps whatever it was already called.
                let kept = StateMeta::load_file(&name, &index)
                    .map(|meta| meta.name)
                    .unwrap_or_default();
                let meta = StateMeta::new(
                    &emu.runtime.cpu.clock.bus.cart,
                    kept,
                    self.roms.playtime(&name),
                );

                if let Err(err) = meta.save_file(&name, &index) {
                    log::warn!("Failed save state meta: {err}");
                }

                let shot = shot::save(
                    &name,
                    &index,
                    emu.get_framebuffer(),
                    RenderConfig::WIDTH,
                    RenderConfig::HEIGHT,
                );

                if let Err(err) = shot {
                    log::warn!("Failed save state shot: {err}");
                }

                // The UI lists the states on disk; one just appeared.
                self.frontend.request_update();
                let msg = format!("Saved save state: {index}");
                self.notifications.add(msg);
            }
            SaveStateCmd::Load => {
                let index = index.unwrap_or(self.config.current_load_slot).to_string();
                let save_state = save::read_state(&name, &index);

                let Ok(save_state) = save_state else {
                    log::error!("Failed load save_state: {}", save_state.unwrap_err());
                    return;
                };

                // A state from another dump or revision restores into ROM banks
                // that don't match it, which shows up as the game misbehaving
                // rather than as a failure to load. Say so, but still load: a
                // different revision can be exactly what the user meant.
                if let Ok(meta) = StateMeta::load_file(&name, &index) {
                    let cart = CartId::of(&emu.runtime.cpu.clock.bus.cart);

                    if !meta.belongs_to(&cart) {
                        let msg = format!("Warning: state is from {}", meta.cart.title);
                        self.notifications.add(msg);
                    }
                }

                emu.load_save_state(save_state);
                let colors = self
                    .config
                    .video
                    .interface
                    .get_palette_colors(&self.palettes);
                self.apply_dmg_palette(emu, colors);
                emu.runtime.cpu.clock.bus.io.apu.config = self.config.audio.get_apu_config();

                let msg = format!("Loaded save state: {index}");
                self.notifications.add(msg);
                self.state = AppState::Running;
            }
        }
    }

    pub fn handle_delete_state(&mut self, index: usize) {
        let Some(name) = self.game_name() else {
            log::error!("Failed delete_state: no game loaded");
            return;
        };

        let index = index.to_string();

        if let Err(err) = save::delete_state(&name, &index) {
            log::error!("Failed delete_state: {err}");
            return;
        }

        if let Err(err) = StateMeta::delete_file(&name, &index) {
            log::warn!("Failed delete state meta: {err}");
        }

        if let Err(err) = shot::delete(&name, &index) {
            log::warn!("Failed delete state shot: {err}");
        }

        // The UI lists the states on disk; one just went away.
        self.frontend.request_update();
        self.notifications
            .add(format!("Deleted save state: {index}"));
    }

    /// Touches only the sidecar: the state itself has no idea what it is called.
    pub fn handle_rename_state(&mut self, index: usize, name: String) {
        let Some(game) = self.game_name() else {
            log::error!("Failed rename_state: no game loaded");
            return;
        };

        let index = index.to_string();
        let mut meta = StateMeta::load_file(&game, &index).unwrap_or_default();
        meta.name = name;

        if let Err(err) = meta.save_file(&game, &index) {
            log::error!("Failed rename_state: {err}");
            return;
        }

        self.frontend.request_update();
        let msg = if meta.name.is_empty() {
            format!("Cleared name of save state: {index}")
        } else {
            format!("Renamed save state {index}: {}", meta.name)
        };
        self.notifications.add(msg);
    }
}

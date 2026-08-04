//! What the picture looks like: window scale, fullscreen, shader, palette. Each
//! one writes the config, tells the video layer, and says so on screen.

use crate::app::App;
use crate::frontend::Frontend;
use crate::video::shader::{next_shader_by_name, prev_shader_by_name};
use crate::{PlatformFileDialog, PlatformFileSystem};
use core::emu::config::GbModel;
use core::emu::Emu;
use core::ppu::tile::PixelColor;

impl<FS, FD> App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fn change_scale(&mut self, delta: f32) -> Result<(), String> {
        self.config.video.interface.scale = (self.config.video.interface.scale + delta).max(0.0);
        self.video.set_scale(
            self.config.video.interface.scale as u32,
            self.config.video.interface.scale_mode,
        )?;
        let msg = format!("Scale: {}", self.config.video.interface.scale);
        self.notifications.add(msg);

        Ok(())
    }

    pub fn next_palette(&mut self, emu: &mut Emu) {
        self.config.video.interface.selected_palette_idx = core::move_next_wrapped(
            self.config.video.interface.selected_palette_idx,
            self.palettes.len() - 1,
        );
        self.update_palette(emu);
    }

    pub fn prev_palette(&mut self, emu: &mut Emu) {
        self.config.video.interface.selected_palette_idx = core::move_prev_wrapped(
            self.config.video.interface.selected_palette_idx,
            self.palettes.len() - 1,
        );
        self.update_palette(emu);
    }

    /// Apply the DMG palette to the emulator: the authentic GBC boot-ROM
    /// colorization when the CGB model is selected for a monochrome cart,
    /// otherwise the selected preset `colors`.
    pub(super) fn apply_dmg_palette(&self, emu: &mut Emu, colors: [PixelColor; 4]) {
        let compat = (emu.config.model == Some(GbModel::Cgb))
            .then(|| emu.dmg_compat_palette())
            .flatten();

        let lcd = &mut emu.runtime.cpu.clock.bus.io.ppu.lcd;
        match compat {
            Some(compat) => {
                // Colorized DMG game: force the DMG-compat render path (the CGB
                // path has no DMG-compat handling) so BGP/OBP still permute the
                // assigned colors.
                lcd.set_model(GbModel::Dmg);
                compat.apply(&mut lcd.dmg_palette);
            }
            None => lcd.dmg_palette.set_colors(colors),
        }
    }

    /// Re-apply the DMG palette using the current config (preset colors or the
    /// GBC colorization). Call after the model or preset changes.
    pub(crate) fn refresh_dmg_palette(&self, emu: &mut Emu) {
        let colors = self.config.video.interface.palette_colors(&self.palettes);
        self.apply_dmg_palette(emu, colors);
    }

    pub fn update_palette(&mut self, emu: &mut Emu) {
        let palette = &self.palettes[self.config.video.interface.selected_palette_idx];
        let colors = self.config.video.interface.palette_colors(&self.palettes);
        self.video.overlay.text_color = colors[0];
        self.video.overlay.bg_color = colors[3];
        self.apply_dmg_palette(emu, colors);
        self.frontend.request_update();

        let suffix = if self.config.video.interface.is_palette_inverted {
            " (inv)"
        } else {
            ""
        };
        let msg = format!("Palette: {}{}", palette.name, suffix);
        self.notifications.add(msg);
    }

    pub fn next_shader(&mut self) {
        let (name, _shader) = next_shader_by_name(&self.config.video.render.gl.shader_name);
        self.update_shader(name);
    }

    pub fn prev_shader(&mut self) {
        let (name, _shader) = prev_shader_by_name(&self.config.video.render.gl.shader_name);
        self.update_shader(name);
    }

    pub fn update_shader(&mut self, name: impl Into<String>) {
        self.config.video.render.gl.shader_name = name.into();
        self.video.update_config(&self.config.video);
        self.frontend.request_update();
        self.notifications.add(format!(
            "Shader: {}",
            self.config.video.render.gl.shader_name
        ));
    }

    pub fn toggle_fullscreen(&mut self) {
        self.config.video.interface.is_fullscreen = !self.config.video.interface.is_fullscreen;
        self.video.set_fullscreen(
            self.config.video.interface.is_fullscreen,
            self.config.video.interface.scale_mode,
        );
    }
}

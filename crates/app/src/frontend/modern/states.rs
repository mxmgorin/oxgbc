//! Builds the loaded game's save-state list from the files on disk. A slot is
//! nothing but a state file named after its index, so the disk is the only
//! record of which slots are taken.

use crate::config::{AppConfig, RenderConfig};
use crate::frontend::FrontendCtx;
use crate::state_meta::StateMeta;
use crate::state_shot::StateShot;
use crate::AppConfigFile;
use crate::PlatformFileSystem;
use std::time::SystemTime;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// Stats one file per slot, so it is built only when the app reports a change.
/// `version` tells the UI this is a different build of the view than last time,
/// which is its cue to drop the textures it uploaded from the old one.
pub fn view<FS: PlatformFileSystem>(ctx: &FrontendCtx<'_, FS>, version: u64) -> ui::StatesView {
    let Some(name) = game_name(ctx) else {
        return ui::StatesView {
            version,
            ..Default::default()
        };
    };

    let now = SystemTime::now();
    let mut slots = Vec::new();
    let mut free = None;

    for slot in 0..=AppConfig::MAX_SAVE_SLOT {
        let Some(mtime) = written_at(&name, slot) else {
            free = free.or(Some(slot));
            continue;
        };

        // Only occupied slots are worth a sidecar read, and there are few of them.
        let suffix = slot.to_string();
        let meta = StateMeta::load_file(&name, &suffix).unwrap_or_default();
        let saved = age(now, meta.written_at().unwrap_or(mtime));

        slots.push(ui::StateSlot {
            slot,
            name: meta.name,
            saved,
            // A few KB per slot, unlike the state itself; states written before
            // shots existed have none, and `load_shot` fills those in on demand.
            shot: StateShot::load_file(&name, &suffix).ok().map(into_ui_shot),
        });
    }

    ui::StatesView {
        slots,
        free,
        version,
    }
}

/// The screen of a state written before shots were saved beside them. It is still
/// in the state file — `Lcd::buffer` is serialized along with the rest of the PPU
/// — but only behind a decode of the whole thing, so this runs for one slot at a
/// time, when its sheet opens.
pub fn load_shot<FS: PlatformFileSystem>(
    ctx: &FrontendCtx<'_, FS>,
    slot: usize,
) -> Option<ui::StateShot> {
    let name = game_name(ctx)?;
    let state = AppConfigFile::read_save_state_file(&name, &slot.to_string());

    let state = match state {
        Ok(state) => state,
        Err(err) => {
            log::warn!("Failed load state shot: {err}");
            return None;
        }
    };

    Some(ui::StateShot {
        rgb: state.cpu.clock.bus.io.ppu.lcd.buffer.rgb888(),
        width: RenderConfig::WIDTH,
        height: RenderConfig::HEIGHT,
    })
}

fn into_ui_shot(shot: StateShot) -> ui::StateShot {
    ui::StateShot {
        rgb: shot.rgb,
        width: shot.width as usize,
        height: shot.height as usize,
    }
}

fn game_name<FS: PlatformFileSystem>(ctx: &FrontendCtx<'_, FS>) -> Option<String> {
    ctx.roms
        .get_last_path()
        .and_then(|path| ctx.fs.get_file_name(path))
}

/// `None` for a slot holding no state — a missing file *is* the empty slot.
fn written_at(game: &str, slot: usize) -> Option<SystemTime> {
    let path = AppConfigFile::get_save_state_path(game, &slot.to_string());

    path.metadata().ok()?.modified().ok()
}

/// Coarsest unit that still says something: the list is for telling two states
/// apart, not for reading a clock off.
fn age(now: SystemTime, written: SystemTime) -> String {
    // A file stamped in the future — a clock change, or a state copied in from
    // another machine — has no age to show.
    let Ok(elapsed) = now.duration_since(written) else {
        return "just now".to_owned();
    };
    let secs = elapsed.as_secs();

    if secs < MINUTE {
        "just now".to_owned()
    } else if secs < HOUR {
        format!("{} min ago", secs / MINUTE)
    } else if secs < DAY {
        format!("{} h ago", secs / HOUR)
    } else {
        format!("{} d ago", secs / DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn age_after(secs: u64) -> String {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);

        age(now, SystemTime::UNIX_EPOCH)
    }

    #[test]
    fn ages_step_up_through_the_units() {
        assert_eq!(age_after(0), "just now");
        assert_eq!(age_after(MINUTE - 1), "just now");
        assert_eq!(age_after(MINUTE), "1 min ago");
        assert_eq!(age_after(HOUR - 1), "59 min ago");
        assert_eq!(age_after(HOUR), "1 h ago");
        assert_eq!(age_after(DAY - 1), "23 h ago");
        assert_eq!(age_after(DAY), "1 d ago");
        assert_eq!(age_after(DAY * 30), "30 d ago");
    }

    #[test]
    fn a_state_stamped_in_the_future_has_no_age() {
        let now = SystemTime::UNIX_EPOCH;

        assert_eq!(age(now, now + Duration::from_secs(HOUR)), "just now");
    }
}

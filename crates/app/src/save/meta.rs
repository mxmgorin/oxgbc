//! What the app knows about a save state but the state itself does not: the name
//! the user gave it, when it was written, and which cartridge it came from.
//!
//! Kept in JSON beside the state rather than inside it — `EmuSaveState` is the
//! emulator's business — and per slot rather than per game, so copying a slot's
//! files somewhere else carries its name along.

use crate::library::meta::CartId;
use core::cart::Cart;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const META_EXT: &str = "json";

/// Every field defaults, so a sidecar written by an older build still loads and
/// new fields can be added without a migration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateMeta {
    /// What the user called this state; empty leaves the slot going by its number.
    #[serde(default)]
    pub name: String,
    /// Unix seconds. Kept here rather than read off the file, whose mtime a copy,
    /// a backup restore or an rsync rewrites.
    #[serde(default)]
    pub saved_at: u64,
    #[serde(default)]
    pub cart: CartId,
    /// Wall-clock seconds played by the time this state was written.
    #[serde(default)]
    pub playtime_secs: u64,
}

impl StateMeta {
    /// `name` is passed in rather than taken from the cart: overwriting a slot
    /// keeps whatever it was already called.
    pub fn new(cart: &Cart, name: String, playtime_secs: u64) -> Self {
        Self {
            name,
            saved_at: now_secs(),
            cart: CartId::of(cart),
            playtime_secs,
        }
    }

    /// Whether this state came off `cart`. A state of unknown provenance counts as
    /// a match: every one written before sidecars recorded it is, and they have to
    /// keep loading quietly.
    pub fn belongs_to(&self, cart: &CartId) -> bool {
        self.cart == CartId::default() || self.cart == *cart
    }

    /// When the state was written, from the sidecar if there is one and from the
    /// file itself for slots saved before sidecars existed.
    pub fn written_at(&self) -> Option<SystemTime> {
        (self.saved_at > 0).then(|| UNIX_EPOCH + std::time::Duration::from_secs(self.saved_at))
    }

    pub fn save_file(&self, game: &str, suffix: &str) -> Result<(), String> {
        let path = Self::path(game, suffix);

        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(json.as_bytes()).map_err(|e| e.to_string())
    }

    pub fn load_file(game: &str, suffix: &str) -> Result<Self, String> {
        let data = fs::read_to_string(Self::path(game, suffix)).map_err(|e| e.to_string())?;

        serde_json::from_str(&data).map_err(|e| e.to_string())
    }

    /// A slot with no sidecar is not an error — every state written before this
    /// existed is one.
    pub fn delete_file(game: &str, suffix: &str) -> Result<(), String> {
        let path = Self::path(game, suffix);

        if !path.exists() {
            return Ok(());
        }

        fs::remove_file(path).map_err(|e| e.to_string())
    }

    /// Derived from the state's own path, so the two can never land apart.
    pub fn path(game: &str, suffix: &str) -> PathBuf {
        super::state_path(game, suffix).with_extension(META_EXT)
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidecar_sits_beside_its_state() {
        let state = crate::save::state_path("Zelda.gb", "3");
        let meta = StateMeta::path("Zelda.gb", "3");

        assert_eq!(state.parent(), meta.parent());
        assert_eq!(meta.file_name().unwrap(), "Zelda.gb_3.json");
    }

    #[test]
    fn a_sidecar_without_a_stamp_has_no_time_of_its_own() {
        assert_eq!(StateMeta::default().written_at(), None);

        let stamped = StateMeta {
            saved_at: 1,
            ..Default::default()
        };

        assert_eq!(
            stamped.written_at(),
            Some(UNIX_EPOCH + std::time::Duration::from_secs(1))
        );
    }

    #[test]
    fn a_state_belongs_to_the_cart_it_came_off() {
        let zelda = CartId {
            title: "ZELDA".to_owned(),
            global_checksum: 0x1234,
        };
        let patched = CartId {
            global_checksum: 0x4321,
            ..zelda.clone()
        };
        let meta = StateMeta {
            cart: zelda.clone(),
            ..Default::default()
        };

        assert!(meta.belongs_to(&zelda));
        assert!(!meta.belongs_to(&patched));
        // Written before the sidecar recorded a cart: nothing to disagree with.
        assert!(StateMeta::default().belongs_to(&zelda));
    }

    /// Fields were added over time, so old sidecars must still parse.
    #[test]
    fn an_older_sidecar_loads_with_defaults() {
        let meta: StateMeta = serde_json::from_str(r#"{"name":"before the boss"}"#).unwrap();

        assert_eq!(meta.name, "before the boss");
        assert_eq!(meta.saved_at, 0);
        assert_eq!(meta.cart, CartId::default());
    }
}

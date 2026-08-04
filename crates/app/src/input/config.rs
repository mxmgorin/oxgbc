use crate::cmd::AppCmd;
use crate::input::bindings::InputBindings;
use crate::input::combo::ButtonComboBindings;
use crate::input::gamepad::default_buttons;
use crate::input::keyboard::default_keys;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Bindings {
    pub gamepad: GamepadBindings,
    pub keyboard: InputBindings<sdl2::keyboard::Scancode>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GamepadBindings {
    pub left_trigger: TriggerButtonBinding,
    pub right_trigger: TriggerButtonBinding,
    pub buttons: InputBindings<sdl2::controller::Button>,
    pub combo: ButtonComboBindings,
}

impl Default for GamepadBindings {
    fn default() -> Self {
        Self {
            buttons: default_buttons(),
            left_trigger: TriggerButtonBinding::new(None, 2),
            right_trigger: TriggerButtonBinding::new(None, 5),
            combo: ButtonComboBindings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TriggerButtonBinding {
    pub cmd: Option<AppCmd>,
    pub code: u8,
    pub threshold: i16,
}

impl TriggerButtonBinding {
    pub fn new(cmd: Option<AppCmd>, code: u8) -> Self {
        Self {
            cmd,
            code,
            threshold: 30_000,
        }
    }

    pub fn is_pressed(&self, v: i16) -> bool {
        v > self.threshold
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            keyboard: default_keys(),
            gamepad: GamepadBindings::default(),
        }
    }
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputConfig {
    #[serde(with = "bindings_file")]
    pub bindings: Bindings,
    pub combo_interval: Duration,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            bindings: Bindings::default(),
            combo_interval: Duration::from_millis(500),
        }
    }
}

mod bindings_file {
    use super::*;
    use crate::storage::base_dir;
    use serde::{Deserializer, Serializer};

    const FILE: &str = "bindings.json";

    pub fn serialize<S>(bindings: &Bindings, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let path = base_dir().join(FILE);
        core::save_json_file(&path, bindings)
            .map_err(|_| serde::ser::Error::custom("Failed to save bindings.json"))?;
        serializer.serialize_str(FILE)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bindings, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = base_dir().join(FILE);
        let _path: String = String::deserialize(deserializer)?;
        core::read_json_file(path)
            .map_err(|_| serde::de::Error::custom("Failed to read bindings.json"))
    }
}

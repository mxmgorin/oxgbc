use crate::cmd::{AppCmd, ChangeConfigCmd};
use crate::input::bindings::BindableInput;
use crate::input::config::{GamepadBindings, InputConfig};
use crate::input::gamepad_buttons;
use crate::input::{button_to_str, str_to_button};
use core::emu::state::SaveStateCmd;
use sdl2::controller::Button;
use serde::de::Error;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use std::time::{Duration, Instant};
#[derive(Debug, Clone, Copy)]
pub struct ButtonState {
    pub pressed: bool,
    pub last_pressed: Instant,
    pub button: Button,
}

impl ButtonState {
    fn new(button: Button) -> Self {
        Self {
            button,
            pressed: false,
            last_pressed: Instant::now(),
        }
    }

    fn update(&mut self, is_pressed: bool) {
        if is_pressed && !self.pressed {
            self.last_pressed = Instant::now();
        }

        self.pressed = is_pressed;
    }
}

pub struct ComboHandler {
    states: [ButtonState; Button::COUNT],
}

impl ComboHandler {
    pub fn new() -> Self {
        let mut states = [ButtonState::new(Button::A); Button::COUNT];

        for button in gamepad_buttons() {
            states[button.code()] = ButtonState::new(*button);
        }

        Self { states }
    }

    /// When `button` went down, while it is still down.
    pub fn held_since(&self, button: Button) -> Option<Instant> {
        let state = self.states[button.code()];

        state.pressed.then_some(state.last_pressed)
    }

    pub fn handle(
        &mut self,
        button: Button,
        pressed: bool,
        config: &InputConfig,
    ) -> Option<AppCmd> {
        let state = &mut self.states[button.code()];
        state.update(pressed);
        self.find_combo(&config.bindings.gamepad, button, config.combo_interval)
    }

    fn find_combo(&self, bindings: &GamepadBindings, b1: Button, dur: Duration) -> Option<AppCmd> {
        let c1 = b1.code();

        for c2 in 0..Button::COUNT {
            let idx = ButtonComboBindings::index(c1, c2);

            if let Some(cmd) = bindings.combo.cmds[idx].as_ref() {
                let b2 = Button::from_code(c2)?;

                if self.combo_2(b1, b2, dur) {
                    return Some(cmd.clone());
                }
            }
        }

        None
    }

    /// Generic function to check any 2-button combo
    fn combo_2(&self, b1: Button, b2: Button, dur: Duration) -> bool {
        let state_1 = self.states[b1.code()];
        let state_2 = self.states[b2.code()];

        if state_1.pressed && state_2.pressed {
            let diff = if state_1.last_pressed > state_2.last_pressed {
                state_1.last_pressed.duration_since(state_2.last_pressed)
            } else {
                state_2.last_pressed.duration_since(state_1.last_pressed)
            };

            return diff <= dur;
        }

        false
    }
}

#[derive(Clone, Debug)]
pub struct ButtonCombo {
    pub btn_1: Button,
    pub btn_2: Button,
    pub cmd: AppCmd,
}

impl ButtonCombo {
    pub fn new(btn_1: Button, btn_2: Button, cmd: AppCmd) -> Self {
        Self { btn_1, btn_2, cmd }
    }
}

#[derive(Debug, Clone)]
pub struct ButtonComboBindings {
    cmds: Box<[Option<AppCmd>]>,
}

impl ButtonComboBindings {
    pub fn new() -> Self {
        let n = Button::COUNT;

        Self {
            cmds: vec![None; n * n].into_boxed_slice(),
        }
    }

    #[inline(always)]
    pub fn index(b1: usize, b2: usize) -> usize {
        b1 * Button::COUNT + b2
    }

    pub fn add_cmd(&mut self, b1: Button, b2: Button, cmd: AppCmd) {
        let c1 = b1.code();
        let c2 = b2.code();

        let i1 = Self::index(c1, c2);
        let i2 = Self::index(c2, c1);

        self.cmds[i1] = Some(cmd.clone());
        self.cmds[i2] = Some(cmd);
    }

    /// Takes `cmd` off every pair that reaches it. A sweep of the whole table rather
    /// than a lookup: [`Self::add_cmd`] writes a pair both ways round, and the
    /// defaults deliberately give one action several pairs.
    pub fn clear_cmd(&mut self, cmd: &AppCmd) {
        for bound in self.cmds.iter_mut() {
            if bound.as_ref() == Some(cmd) {
                *bound = None;
            }
        }
    }

    pub fn iter_combos(&self) -> impl Iterator<Item = ButtonCombo> + '_ {
        let n = Button::COUNT;

        (0..n).flat_map(move |code_1| {
            ((code_1 + 1)..n).filter_map(move |code_2| {
                let idx = code_1 * n + code_2;
                let cmd = self.cmds[idx].as_ref()?.clone();

                let btn_1 = Button::from_code(code_1)?;
                let btn_2 = Button::from_code(code_2)?;

                Some(ButtonCombo { btn_1, btn_2, cmd })
            })
        })
    }
}

impl Default for ButtonComboBindings {
    fn default() -> Self {
        let mut bindings = Self::new();

        bindings.add_cmd(
            Button::Back,
            Button::B,
            AppCmd::ChangeConfig(ChangeConfigCmd::PrevShader),
        );
        bindings.add_cmd(
            Button::Guide,
            Button::B,
            AppCmd::ChangeConfig(ChangeConfigCmd::PrevShader),
        );

        bindings.add_cmd(
            Button::Back,
            Button::A,
            AppCmd::ChangeConfig(ChangeConfigCmd::NextShader),
        );
        bindings.add_cmd(
            Button::Guide,
            Button::A,
            AppCmd::ChangeConfig(ChangeConfigCmd::NextShader),
        );

        bindings.add_cmd(Button::Start, Button::Back, AppCmd::ToggleMenu);
        bindings.add_cmd(Button::Start, Button::Guide, AppCmd::ToggleMenu);
        // A handheld never sees Start + Select: there it closes whatever is running.
        bindings.add_cmd(Button::Back, Button::Y, AppCmd::ToggleMenu);
        bindings.add_cmd(Button::Guide, Button::Y, AppCmd::ToggleMenu);

        bindings.add_cmd(
            Button::Guide,
            Button::X,
            AppCmd::ChangeConfig(ChangeConfigCmd::InvertPalette),
        );
        bindings.add_cmd(
            Button::Back,
            Button::X,
            AppCmd::ChangeConfig(ChangeConfigCmd::InvertPalette),
        );

        bindings.add_cmd(
            Button::LeftShoulder,
            Button::Back,
            AppCmd::SaveState(SaveStateCmd::Load, None),
        );
        bindings.add_cmd(
            Button::RightShoulder,
            Button::Back,
            AppCmd::SaveState(SaveStateCmd::Create, None),
        );
        bindings.add_cmd(
            Button::LeftShoulder,
            Button::Guide,
            AppCmd::SaveState(SaveStateCmd::Load, None),
        );
        bindings.add_cmd(
            Button::RightShoulder,
            Button::Guide,
            AppCmd::SaveState(SaveStateCmd::Create, None),
        );

        bindings.add_cmd(
            Button::DPadUp,
            Button::Start,
            AppCmd::ChangeConfig(ChangeConfigCmd::Volume(0.1)),
        );
        bindings.add_cmd(
            Button::DPadDown,
            Button::Start,
            AppCmd::ChangeConfig(ChangeConfigCmd::Volume(-0.1)),
        );
        bindings.add_cmd(
            Button::DPadLeft,
            Button::Start,
            AppCmd::ChangeConfig(ChangeConfigCmd::DecSaveAndLoadSlots),
        );
        bindings.add_cmd(
            Button::DPadRight,
            Button::Start,
            AppCmd::ChangeConfig(ChangeConfigCmd::IncSaveAndLoadSlots),
        );

        bindings
    }
}

impl Serialize for ButtonCombo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("ButtonCombo", 3)?;
        state.serialize_field("btn_1", &button_to_str(self.btn_1))?;
        state.serialize_field("btn_2", &button_to_str(self.btn_2))?;
        state.serialize_field("cmd", &self.cmd)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for ButtonCombo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct StrButtonCombo {
            btn_1: String,
            btn_2: String,
            cmd: AppCmd,
        }

        let combo = StrButtonCombo::deserialize(deserializer)?;
        let b1 = str_to_button(&combo.btn_1)
            .ok_or_else(|| D::Error::custom(format!("Unknown button: {}", combo.btn_1)))?;
        let b2 = str_to_button(&combo.btn_2)
            .ok_or_else(|| D::Error::custom(format!("Unknown button: {}", combo.btn_2)))?;

        Ok(ButtonCombo {
            btn_1: b1,
            btn_2: b2,
            cmd: combo.cmd,
        })
    }
}

impl Serialize for ButtonComboBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let combos: Vec<_> = self.iter_combos().collect();
        combos.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ButtonComboBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let combos = Vec::<ButtonCombo>::deserialize(deserializer)?;
        let mut bindings = ButtonComboBindings::new();

        for combo in combos {
            bindings.add_cmd(combo.btn_1, combo.btn_2, combo.cmd);
        }

        Ok(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The defaults give one action several pairs, because not every pad has a Guide
    /// button. Rebinding replaces, so all of them have to go.
    #[test]
    fn clearing_an_action_takes_every_pair_that_reached_it() {
        let mut bindings = ButtonComboBindings::default();
        let menu = AppCmd::ToggleMenu;

        assert!(bindings.iter_combos().any(|combo| combo.cmd == menu));

        bindings.clear_cmd(&menu);

        assert!(
            !bindings.iter_combos().any(|combo| combo.cmd == menu),
            "both the Back and the Guide pair"
        );
        assert!(
            bindings.iter_combos().count() > 0,
            "and nothing else was touched"
        );
    }

    /// A pair is stored both ways round, so the order it was pressed in cannot matter.
    #[test]
    fn a_pair_reads_the_same_either_way_round() {
        let mut bindings = ButtonComboBindings::new();
        bindings.add_cmd(Button::LeftShoulder, Button::A, AppCmd::ToggleMenu);

        let i1 = ButtonComboBindings::index(Button::LeftShoulder.code(), Button::A.code());
        let i2 = ButtonComboBindings::index(Button::A.code(), Button::LeftShoulder.code());

        assert_eq!(bindings.cmds[i1], Some(AppCmd::ToggleMenu));
        assert_eq!(bindings.cmds[i2], Some(AppCmd::ToggleMenu));
    }
}

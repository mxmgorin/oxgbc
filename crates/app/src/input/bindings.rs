use crate::cmd::AppCmd;
use core::auxiliary::joypad::JoypadButton;
use serde::de::{Error, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub trait BindableInput: Copy {
    const COUNT: usize;

    fn code(self) -> usize;
    fn from_code(index: usize) -> Option<Self>;
    fn name(self) -> &'static str;
    fn from_name(name: &str) -> Option<Self>;
    fn kind(self) -> InputKind;

    /// Whether this input backs out of a rebinding instead of being captured by it.
    /// Only the keyboard has one to spare: on a pad every button is worth binding,
    /// and a capture landing on the wrong one is undone by capturing again.
    fn is_cancel(self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum InputKind {
    Keyboard,
    Gamepad,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InputIndex(usize);

impl From<usize> for InputIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl InputIndex {
    #[inline(always)]
    pub fn new<I: BindableInput>(input: I, pressed: bool) -> Self {
        Self(input.code() * 2 + if pressed { 0 } else { 1 })
    }

    #[inline(always)]
    pub fn index(self) -> usize {
        self.0
    }

    #[inline(always)]
    pub fn pressed(self) -> bool {
        (self.0 % 2) == 0
    }

    #[inline(always)]
    pub fn into_input<I: BindableInput>(self) -> Option<I> {
        let input_index = self.0 / 2;
        let input = I::from_code(input_index)?;

        Some(input)
    }
}

#[derive(Debug, Clone)]
pub struct InputBindings<K: BindableInput> {
    cmds: Vec<Option<AppCmd>>,
    _marker: std::marker::PhantomData<K>,
}

impl<I: BindableInput> InputBindings<I> {
    #[inline(always)]
    pub fn cmd(&self, input: I, pressed: bool) -> Option<&AppCmd> {
        self.cmds
            .get(InputIndex::new(input, pressed).index())
            .and_then(|x| x.as_ref())
    }

    pub fn desc(&self, cmd: &AppCmd) -> String {
        let desc = self
            .inputs(cmd)
            .into_iter()
            .map(|(i, _)| i.name())
            .collect::<Vec<_>>()
            .join(", ");

        if desc.is_empty() {
            "None".to_string()
        } else {
            desc
        }
    }

    pub fn inputs(&self, cmd: &AppCmd) -> Vec<(I, bool)> {
        let mut inputs = Vec::with_capacity(2);

        for (i, item) in self.cmds.iter().enumerate() {
            if let Some(item) = item {
                if item == cmd {
                    let index: InputIndex = i.into();

                    if let Some(input) = index.into_input() {
                        inputs.push((input, index.pressed()));
                    }
                }
            }
        }

        inputs
    }

    #[inline(always)]
    pub fn bind_cmd(&mut self, input: I, pressed: bool, cmd: AppCmd) {
        let i = InputIndex::new(input, pressed).index();
        self.cmds[i] = Some(cmd);
    }

    pub fn unbind(&mut self, input: I, pressed: bool) {
        let i = InputIndex::new(input, pressed).index();
        self.cmds[i] = None;
    }

    /// Takes `cmd` off every input that reaches it. Rebinding replaces rather than
    /// adds: leaving the old input in place would have two keys doing one thing and
    /// no way to see it from the settings row, which shows one binding.
    pub fn clear_cmd(&mut self, cmd: &AppCmd) {
        for bound in self.cmds.iter_mut() {
            if bound.as_ref() == Some(cmd) {
                *bound = None;
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (I, bool, &AppCmd)> {
        self.cmds.iter().enumerate().filter_map(|(i, entry)| {
            let cmd = entry.as_ref()?;
            let index: InputIndex = i.into();
            let input = index.into_input()?;

            Some((input, index.pressed(), cmd))
        })
    }

    pub fn bind_btn(&mut self, input: I, btn: JoypadButton) {
        self.bind_cmd(input, true, AppCmd::PressButton(btn));
        self.bind_cmd(input, false, AppCmd::ReleaseButton(btn));
    }

    pub fn bind_macro<B: Into<Box<[JoypadButton]>> + Clone>(&mut self, i: I, b: B) {
        self.bind_cmd(i, true, AppCmd::new_macro_buttons(b.clone(), true));
        self.bind_cmd(i, false, AppCmd::new_macro_buttons(b, false));
    }
}

impl<K: BindableInput> Default for InputBindings<K> {
    fn default() -> Self {
        Self {
            cmds: vec![None; K::COUNT * 2],
            _marker: std::marker::PhantomData,
        }
    }
}

impl<K> Serialize for InputBindings<K>
where
    K: BindableInput,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map_ser = serializer.serialize_map(None)?;

        for (sc, pressed, cmd) in self.iter() {
            let key = format!(
                "{}.{}",
                sc.name(),
                if pressed { "pressed" } else { "released" }
            );
            map_ser.serialize_entry(&key, cmd)?;
        }

        map_ser.end()
    }
}

impl<'de, K> Deserialize<'de> for InputBindings<K>
where
    K: BindableInput,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct KeyBindingsVisitor<K: BindableInput>(std::marker::PhantomData<K>);

        impl<'de, K> Visitor<'de> for KeyBindingsVisitor<K>
        where
            K: BindableInput,
        {
            type Value = InputBindings<K>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a map of key.state (pressed/released) to AppCmd")
            }

            fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut bindings = InputBindings::<K>::default();

                while let Some((key, cmd)) = access.next_entry::<String, AppCmd>()? {
                    let mut parts = key.split('.');
                    let key_name = parts.next().unwrap_or("");
                    let state_str = parts.next().unwrap_or("pressed");

                    let pressed = match state_str {
                        "pressed" => true,
                        "released" => false,
                        _ => return Err(M::Error::custom(format!("Invalid state in key: {key}"))),
                    };

                    let sc = K::from_name(key_name)
                        .ok_or_else(|| M::Error::custom(format!("Unknown key: {key_name}")))?;

                    bindings.bind_cmd(sc, pressed, cmd);
                }

                Ok(bindings)
            }
        }

        deserializer.deserialize_map(KeyBindingsVisitor::<K>(std::marker::PhantomData))
    }
}

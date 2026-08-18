pub mod buffer;
pub mod factory;
pub mod files;
pub mod handler;
pub mod item;
pub mod roms;

use self::buffer::MenuBuffer;
use self::item::AppMenuItem;
use crate::cmd::{AppCmd, BindInputCmd};
use crate::config::AppConfig;
use crate::input::bindings::BindableInput;
use crate::library::RomsState;
use std::mem;

pub const MAX_MENU_ITEMS_PER_PAGE: usize = 12;
pub const MAX_MENU_ITEM_CHARS: usize = 22;

pub struct AppMenu {
    prev_items: Vec<Box<[AppMenuItem]>>,
    items: Box<[AppMenuItem]>,
    selected_index: usize,
    buffer: MenuBuffer,
    updated: bool,
    sub_buffer: MenuBuffer,
}

impl AppMenu {
    pub fn new(roms: &RomsState) -> Self {
        Self {
            prev_items: Vec::with_capacity(4),
            items: factory::start_menu(roms),
            selected_index: 0,
            buffer: MenuBuffer::default(),
            updated: true,
            sub_buffer: Default::default(),
        }
    }

    /// Its bind screen is a menu item like any other: the selected one waiting.
    pub fn is_waiting_input(&self) -> bool {
        matches!(
            self.items.get(self.selected_index),
            Some(AppMenuItem::WaitInput(_))
        )
    }

    pub fn handle_input<I: BindableInput>(&mut self, input: I, pressed: bool) -> Option<AppCmd> {
        let item = self.items.get(self.selected_index).unwrap();

        let cmd = match item {
            AppMenuItem::WaitInput(btns) => {
                let btns = btns.to_owned();
                self.back();

                Some(AppCmd::BindInput(BindInputCmd::new(input, pressed, btns)))
            }
            _ => None,
        };

        cmd
    }

    #[inline(always)]
    pub fn request_update(&mut self) {
        self.updated = true;
    }

    #[inline(always)]
    pub fn needs_update(&self) -> bool {
        self.updated
    }

    pub fn items(&mut self, config: &AppConfig, roms: &RomsState) -> (&[&str], bool) {
        let updated = self.updated;
        self.updated = false;

        if updated {
            self.buffer.clear();
            self.sub_buffer.clear();

            for (i, item) in self.items.iter_mut().enumerate() {
                if let Some(sub_items) = item.items() {
                    for sub_item in sub_items.iter() {
                        self.sub_buffer.add(sub_item);
                    }

                    return (self.sub_buffer.get(), updated);
                } else {
                    let line = item.to_string(config, roms);
                    if i == self.selected_index {
                        self.buffer.add(format!("◀{line}▶"));
                    } else {
                        self.buffer.add(line.to_string());
                    }
                }
            }
        } else if !self.sub_buffer.is_empty() {
            return (self.sub_buffer.get(), updated);
        }

        (self.buffer.get(), updated)
    }

    fn next_items(&mut self, items: Box<[AppMenuItem]>) {
        self.updated = true;
        let prev = mem::replace(&mut self.items, items);
        self.selected_index = 0;
        self.prev_items.push(prev);
    }
}

pub fn menu_toggle(enabled: bool) -> &'static str {
    if enabled {
        "●"
    } else {
        "○"
    }
}

pub trait SubMenu {
    fn iter(&self) -> Box<dyn Iterator<Item = String> + '_>;
    fn move_up(&mut self);
    fn move_down(&mut self);
    fn move_left(&mut self) -> Option<AppCmd>;
    fn move_right(&mut self) -> Option<AppCmd>;
    fn select(&mut self, _config: &AppConfig) -> (Option<AppCmd>, bool);
    fn next_page(&mut self);
    fn prev_page(&mut self);
}

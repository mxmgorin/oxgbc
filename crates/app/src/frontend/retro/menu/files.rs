use super::{SubMenu, MAX_MENU_ITEMS_PER_PAGE, MAX_MENU_ITEM_CHARS};
use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::file_browser::{FileBrowser, FILE_BROWSER_BACK_ITEM};
use crate::video::truncate_text;
use std::path::Path;

#[derive(Debug)]
pub struct FilesMenu {
    fb: FileBrowser,
}

impl FilesMenu {
    pub fn new(last_path: Option<impl AsRef<Path>>) -> Self {
        let extensions = &["gb", "gbc"];

        Self {
            fb: if let Some(last_path) = last_path {
                FileBrowser::new(last_path, MAX_MENU_ITEMS_PER_PAGE, extensions).unwrap()
            } else {
                FileBrowser::new(".", MAX_MENU_ITEMS_PER_PAGE, extensions).unwrap()
            },
        }
    }
}

impl SubMenu for FilesMenu {
    fn get_iterator<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a> {
        let selected = self.fb.get_selected();

        Box::new(self.fb.get_page_entries().iter().map(move |path| {
            let mut name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if path.is_dir() || path == Path::new(FILE_BROWSER_BACK_ITEM) {
                name = format!("/{name}");
            }

            let name = truncate_text(&name, MAX_MENU_ITEM_CHARS);

            if let Some(selected) = selected {
                if selected == path {
                    format!("◀{name}▶")
                } else {
                    name
                }
            } else {
                name
            }
        }))
    }

    fn move_up(&mut self) {
        self.fb.up();
    }

    fn move_down(&mut self) {
        self.fb.down();
    }

    fn move_left(&mut self) -> Option<AppCmd> {
        self.fb.prev_page();

        None
    }

    fn move_right(&mut self) -> Option<AppCmd> {
        self.fb.next_page();

        None
    }

    fn select(&mut self, _config: &AppConfig) -> (Option<AppCmd>, bool) {
        if let Some(selected) = self.fb.get_selected() {
            return if selected.is_dir() || selected == Path::new(FILE_BROWSER_BACK_ITEM) {
                if let Err(err) = self.fb.enter() {
                    log::error!("{err:?}");
                }

                (
                    Some(AppCmd::SetFileBrowsePath(self.fb.current_dir.clone())),
                    false,
                )
            } else {
                (Some(AppCmd::LoadFile(selected.clone())), false)
            };
        }

        (None, false)
    }

    fn next_page(&mut self) {
        self.fb.next_page();
    }

    fn prev_page(&mut self) {
        self.fb.prev_page();
    }
}

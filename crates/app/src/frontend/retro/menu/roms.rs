use super::{SubMenu, MAX_MENU_ITEMS_PER_PAGE, MAX_MENU_ITEM_CHARS};
use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::library::RomsState;
use crate::video::truncate_text;
use crate::PlatformFileSystem;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RomMenuItem {
    pub name: String,
    pub path: PathBuf,
}

impl RomMenuItem {
    pub fn new(path: impl Into<PathBuf>, filesystem: &impl PlatformFileSystem) -> Option<Self> {
        let path = path.into();
        let name = filesystem.file_name(&path)?;

        Some(Self {
            name: truncate_text(&name, MAX_MENU_ITEM_CHARS),
            path,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RomsMenu {
    all_items: Box<[RomMenuItem]>, // all ROMs
    items: Box<[RomMenuItem]>,     // current page items (plus nav items)
    selected_index: usize,
    current_page: usize,
}

impl RomsMenu {
    fn update_page(&mut self) {
        let prev_len = self.items.len();
        let total_pages = self
            .all_items
            .len()
            .div_ceil(MAX_MENU_ITEMS_PER_PAGE)
            .max(1);
        let start = self.current_page * MAX_MENU_ITEMS_PER_PAGE;
        let end = usize::min(start + MAX_MENU_ITEMS_PER_PAGE, self.all_items.len());
        let mut page_items: Vec<RomMenuItem> = self.all_items[start..end].to_vec();

        page_items.push(RomMenuItem {
            name: format!("Page ({}/{})", self.current_page + 1, total_pages),
            path: Default::default(),
        });

        page_items.push(RomMenuItem {
            name: "Back".to_string(),
            path: Default::default(),
        });

        self.items = page_items.into_boxed_slice();

        if prev_len != self.items.len() {
            self.selected_index = self.items.len() - 2;
        }
    }

    pub fn from_loaded(fs: &impl PlatformFileSystem, roms: &RomsState) -> Self {
        let mut all_items = Vec::with_capacity(12);

        if let Some(iter) = roms.iter_loaded(fs) {
            for path in iter {
                if let Some(item) = RomMenuItem::new(path, fs) {
                    all_items.push(item);
                }
            }
        }

        all_items.sort_by(|a, b| a.name.cmp(&b.name));

        let mut menu = Self {
            items: Box::new([]),
            all_items: all_items.into_boxed_slice(),
            selected_index: 0,
            current_page: 0,
        };
        menu.update_page();

        menu
    }

    pub fn from_opened(fs: &impl PlatformFileSystem, roms: &RomsState) -> Self {
        let mut all_items = Vec::with_capacity(12);

        for path in roms.iter_opened() {
            if let Some(item) = RomMenuItem::new(path, fs) {
                all_items.push(item);
            }
        }

        let mut menu = Self {
            items: Box::new([]),
            all_items: all_items.into_boxed_slice(),
            selected_index: 0,
            current_page: 0,
        };
        menu.update_page();

        menu
    }
}

impl SubMenu for RomsMenu {
    fn iter<'a>(&'a self) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(self.items.iter().enumerate().map(move |(i, line)| {
            if i == self.selected_index {
                format!("◀{}▶", line.name)
            } else {
                line.name.clone()
            }
        }))
    }

    fn move_up(&mut self) {
        self.selected_index = core::move_prev_wrapped(self.selected_index, self.items.len() - 1);
    }

    fn move_down(&mut self) {
        self.selected_index = core::move_next_wrapped(self.selected_index, self.items.len() - 1);
    }

    fn move_left(&mut self) -> Option<AppCmd> {
        self.prev_page();

        None
    }

    fn move_right(&mut self) -> Option<AppCmd> {
        self.next_page();

        None
    }

    fn select(&mut self, _config: &AppConfig) -> (Option<AppCmd>, bool) {
        let item = &self.items[self.selected_index];

        if item.name.starts_with("Back") {
            return (None, true);
        }

        (Some(AppCmd::LoadFile(item.path.clone())), false)
    }

    fn next_page(&mut self) {
        let total_pages = self.all_items.len().div_ceil(MAX_MENU_ITEMS_PER_PAGE);
        if self.current_page + 1 < total_pages {
            self.current_page += 1;
            self.update_page();
        }
    }

    fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.update_page();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::frontend::retro::menu::roms::{RomMenuItem, RomsMenu};
    use crate::frontend::retro::menu::SubMenu;
    use crate::PlatformFileSystem;
    use std::path::Path;

    pub struct TestFilesystem;

    impl PlatformFileSystem for TestFilesystem {
        fn file_name(&self, path: &Path) -> Option<String> {
            path.file_stem()?.to_str().map(|x| x.to_string())
        }

        fn read_file_bytes(&self, _path: &Path) -> Option<Box<[u8]>> {
            None
        }

        fn read_dir(&self, _path: &Path) -> Result<Vec<String>, String> {
            Ok(vec![])
        }

        fn can_split_paths(&self) -> bool {
            true
        }
    }

    #[test]
    pub fn iter() {
        let filesystem = TestFilesystem;
        let roms = RomsMenu {
            all_items: Box::new([]),
            items: vec![
                RomMenuItem::new("1", &filesystem).unwrap(),
                RomMenuItem::new("2", &filesystem).unwrap(),
                RomMenuItem::new("3", &filesystem).unwrap(),
            ]
            .into_boxed_slice(),
            selected_index: 0,
            current_page: 0,
        };
        let mut iter = roms.iter();

        assert!(iter.next().is_some());
        assert!(iter.next().is_some());
        assert!(iter.next().is_some());
        assert!(iter.next().is_none());
    }
}

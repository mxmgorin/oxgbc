use std::fs;
use std::path::{Path, PathBuf};

pub const FILE_BROWSER_BACK_ITEM: &str = "/..[up]";

#[derive(Debug)]
pub struct FileBrowser {
    pub current_dir: PathBuf,
    entries: Vec<PathBuf>,
    selected_index: usize,
    page_size: usize,
    extensions: &'static [&'static str],
}

impl FileBrowser {
    /// An empty `extensions` walks folders alone — nothing else can be picked, so
    /// nothing else is worth showing.
    pub fn new<P: AsRef<Path>>(
        path: P,
        page_size: usize,
        extensions: &'static [&'static str],
    ) -> std::io::Result<Self> {
        let current_dir = path.as_ref().to_path_buf().canonicalize()?;
        let mut fm = FileBrowser {
            current_dir,
            entries: Vec::new(),
            selected_index: 0,
            page_size,
            extensions,
        };

        fm.refresh_entries()?;

        Ok(fm)
    }

    /// Points at a row, for a caller that keeps the selection itself and only needs
    /// the walking.
    pub fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            self.selected_index = index;
        }
    }

    pub fn has_parent(&self) -> bool {
        self.current_dir.parent().is_some()
    }

    pub fn back(&mut self) -> std::io::Result<()> {
        if let Some(parent) = self.current_dir.parent() {
            self.current_dir = parent.to_path_buf();
            self.selected_index = 0;
            self.refresh_entries()?;
        }

        Ok(())
    }

    pub fn enter(&mut self) -> std::io::Result<Option<PathBuf>> {
        if let Some(selected) = self.entries.get(self.selected_index) {
            if selected == Path::new(FILE_BROWSER_BACK_ITEM) {
                self.back()?;
            } else if selected.is_dir() {
                self.current_dir = selected.clone();
                self.selected_index = 0;
                self.refresh_entries()?;
            } else {
                return Ok(Some(selected.to_owned()));
            }
        }

        Ok(None)
    }

    /// Move selection up by one, wrap to last if at first
    pub fn up(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        if self.selected_index == 0 {
            self.selected_index = self.entries.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    /// Move selection down by one, wrap to first if at last
    pub fn down(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        self.selected_index = (self.selected_index + 1) % self.entries.len();
    }

    /// Move selection up by one page, wrapping around
    pub fn next_page(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        if self.selected_index < self.page_size {
            // Wrap around to last page (last partial page included)
            let remainder = self.entries.len() % self.page_size;
            let last_page_start = if remainder == 0 {
                self.entries.len() - self.page_size
            } else {
                self.entries.len() - remainder
            };
            self.selected_index = last_page_start
                + (self.selected_index % self.page_size).min(remainder.saturating_sub(1));
        } else {
            self.selected_index -= self.page_size;
        }
    }

    /// Move selection down by one page, wrapping around
    pub fn prev_page(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let next = self.selected_index + self.page_size;
        if next >= self.entries.len() {
            // Wrap back to first page
            self.selected_index %= self.page_size;
        } else {
            self.selected_index = next;
        }
    }

    pub fn get_entries(&self) -> &[PathBuf] {
        &self.entries
    }

    pub fn get_page_entries(&self) -> &[PathBuf] {
        let page_start = (self.selected_index / self.page_size) * self.page_size;
        let page_end = usize::min(page_start + self.page_size, self.entries.len());
        &self.entries[page_start..page_end]
    }

    pub fn get_selected(&self) -> Option<&PathBuf> {
        self.entries.get(self.selected_index)
    }

    fn refresh_entries(&mut self) -> std::io::Result<()> {
        let mut entries: Vec<PathBuf> = fs::read_dir(&self.current_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|path| {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('.') {
                        return false; // skip hidden files/folders
                    }
                }

                if path.is_dir() {
                    true
                } else if self.extensions.is_empty() {
                    false
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    // check if extension matches one of the filters (case insensitive)
                    self.extensions.iter().any(|f| f.eq_ignore_ascii_case(ext))
                } else {
                    false
                }
            })
            .collect();

        // First by "is_dir" (dirs first), then lexicographically
        entries.sort_by(|a, b| match (a.is_dir(), b.is_dir()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .file_name()
                .unwrap_or_default()
                .cmp(b.file_name().unwrap_or_default()),
        });

        // Insert the "back" item at the top
        entries.insert(0, PathBuf::from(FILE_BROWSER_BACK_ITEM));
        self.entries = entries;

        if self.selected_index >= self.entries.len() {
            self.selected_index = 0;
        }

        Ok(())
    }
}

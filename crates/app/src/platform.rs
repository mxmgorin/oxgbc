//! What the app cannot do for itself and every device does differently: reading
//! files and asking the user to pick one. Each build supplies its own pair —
//! desktop, Android, web — and the app holds them as [`AppPlatform`].

use std::path::Path;

pub trait PlatformFileDialog {
    fn select_file(&mut self, title: &str, filter: (&[&str], &str)) -> Option<String>;
    fn select_dir(&mut self, title: &str) -> Option<String>;

    /// Whether this platform's own picker can be reached the way the app is driven.
    /// Android's is a system screen a gamepad walks like any other; a desktop dialog
    /// wants a pointer, and a build without one has nothing at all — those fall back
    /// to walking storage inside the app.
    fn is_navigable(&self) -> bool {
        false
    }
}

pub trait PlatformFileSystem {
    fn get_file_name(&self, path: &Path) -> Option<String>;
    fn read_file_bytes(&self, path: &Path) -> Option<Box<[u8]>>;
    fn read_dir(&self, path: &Path) -> Result<Vec<String>, String>;
    fn can_split_paths(&self) -> bool;
}

pub struct AppPlatform<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fs: FS,
    pub fd: FD,
}

impl<FS, FD> AppPlatform<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fn new(fs: FS, fd: FD) -> Self {
        Self { fs, fd }
    }
}

/// For a build with no picker at all: nothing is ever chosen, so the app walks
/// storage itself.
pub struct EmptyFileDialog;

impl PlatformFileDialog for EmptyFileDialog {
    fn select_file(&mut self, _title: &str, _filter: (&[&str], &str)) -> Option<String> {
        None
    }

    fn select_dir(&mut self, _title: &str) -> Option<String> {
        None
    }
}

use crate::java::{file_name, read_dir, read_uri_bytes};
use app::PlatformFileSystem;
use std::path::Path;

pub struct AndroidFileSystem;

impl PlatformFileSystem for AndroidFileSystem {
    fn file_name(&self, path: &Path) -> Option<String> {
        let path = path.to_str()?;

        file_name(path)
    }

    fn read_file_bytes(&self, path: &Path) -> Option<Box<[u8]>> {
        let path = path.to_str()?;

        read_uri_bytes(path).map(|x| x.into_boxed_slice())
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<String>, String> {
        let path = path.to_str();

        let Some(path) = path else {
            return Ok(vec![]);
        };

        let Some(items) = read_dir(path) else {
            return Ok(vec![]);
        };

        Ok(items)
    }

    fn can_split_paths(&self) -> bool {
        false
    }
}

use app::{AppPlatform, PlatformFileSystem};
use std::path::Path;
use std::{env, fs};

fn main() {
    let env = env_logger::Env::default()
        .filter_or("OXGBC_LOG_LEVEL", "info")
        .write_style_or("OXGBC_LOG_STYLE", "always");
    env_logger::init_from_env(env);
    log::info!("Starting desktop app");
    let args: Vec<String> = env::args().collect();

    #[cfg(feature = "file-dialog")]
    let fd = DesktopFileDialog;

    #[cfg(not(feature = "file-dialog"))]
    let fd = app::EmptyFileDialog;

    app::run(args, AppPlatform::new(DesktopFileSystem, fd));
}

#[cfg(feature = "file-dialog")]
pub struct DesktopFileDialog;

#[cfg(feature = "file-dialog")]
impl app::PlatformFileDialog for DesktopFileDialog {
    fn select_file(&mut self, title: &str, filter: (&[&str], &str)) -> Option<String> {
        tinyfiledialogs::open_file_dialog(title, "", Some(filter))
    }

    fn select_dir(&mut self, title: &str) -> Option<String> {
        tinyfiledialogs::select_folder_dialog(title, "")
    }
}

pub struct DesktopFileSystem;

impl PlatformFileSystem for DesktopFileSystem {
    fn file_name(&self, path: &Path) -> Option<String> {
        path.file_name()?.to_str().map(|x| x.to_string())
    }

    fn read_file_bytes(&self, path: &Path) -> Option<Box<[u8]>> {
        core::read_bytes(path).ok()
    }

    fn read_dir(&self, path: &Path) -> Result<Vec<String>, String> {
        let dir = fs::read_dir(path).map_err(|e| e.to_string())?;

        let files: Vec<String> = dir
            .filter_map(|dir| {
                if let Ok(entry) = dir {
                    let path = entry.path();
                    let path = path
                        .into_os_string()
                        .into_string()
                        .map_err(|e| format!("{e:?}"));

                    let Ok(path) = path else {
                        return None;
                    };

                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    }

    fn can_split_paths(&self) -> bool {
        true
    }
}

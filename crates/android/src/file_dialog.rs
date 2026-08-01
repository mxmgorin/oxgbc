use crate::file_system::AndroidFileSystem;
use crate::java::{show_android_directory_picker, show_android_file_picker};
use app::PlatformFileDialog;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct AndroidFileDialog;
pub static PICKED_FILE_URI: Mutex<Option<Option<String>>> = Mutex::new(None);
pub static PICKED_DIR_URI: Mutex<Option<Option<String>>> = Mutex::new(None);
const TIMEOUT: Duration = Duration::from_secs(180);

impl PlatformFileDialog for AndroidFileDialog {
    fn select_file(&mut self, _title: &str, _filter: (&[&str], &str)) -> Option<String> {
        log::info!("select_file");
        show_android_file_picker();

        let uri = AndroidFileSystem::wait(&PICKED_FILE_URI);
        log::debug!("select_file: {uri:?}");

        uri
    }

    /// A system screen, driven by whatever drives the rest of the device.
    fn is_navigable(&self) -> bool {
        true
    }

    fn select_dir(&mut self, _title: &str) -> Option<String> {
        log::info!("select_dir");
        show_android_directory_picker();

        let uri = AndroidFileSystem::wait(&PICKED_DIR_URI);
        log::debug!("select_dir: {uri:?}");

        uri
    }
}

impl AndroidFileSystem {
    pub fn wait(v: &Mutex<Option<Option<String>>>) -> Option<String> {
        let start = Instant::now();

        loop {
            if let Some(uri) = v.lock().unwrap().take() {
                return uri;
            }

            if start.elapsed() > TIMEOUT {
                return None;
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

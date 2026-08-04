use crate::storage::base_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatterySave {
    pub ram_bytes: Box<[u8]>,
}

impl BatterySave {
    pub fn from_bytes(bytes: Box<[u8]>) -> Self {
        Self { ram_bytes: bytes }
    }

    pub fn save_file(&self, name: &str) -> std::io::Result<()> {
        let path = Self::generate_path(name);

        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = File::create(path)?;
        file.write_all(&self.ram_bytes)?;

        Ok(())
    }

    pub fn load_file(name: &str) -> std::io::Result<Self> {
        let path = Self::generate_path(name);
        let mut file = File::open(path)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        Ok(Self {
            ram_bytes: buffer.into_boxed_slice(),
        })
    }

    pub fn generate_path(name: &str) -> PathBuf {
        base_dir().join("saves").join(format!("{name}.sav"))
    }
}

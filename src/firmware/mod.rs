mod realtek;

use std::future::Future;
use std::path::{Path, PathBuf};
use tracing::error;
pub use realtek::RealTekFirmwareLoader;

pub trait FileProvider {
    fn get_file(&self, name: &str) -> impl Future<Output=Option<Vec<u8>>> + Send;
}

#[derive(Debug, Clone)]
pub struct FolderFileProvider {
    folder: PathBuf
}

impl FolderFileProvider {
    pub fn new<P: AsRef<Path>>(folder: P) -> Self {
        Self { folder: folder.as_ref().to_path_buf() }
    }
}

impl FileProvider for FolderFileProvider {
    async fn get_file(&self, file_name: &str) -> Option<Vec<u8>> {
        let path = self.folder.join(file_name);
        tokio::fs::read(path)
            .await
            .map_err(|err| error!("Failed to read file {}: {:?}", file_name, err))
            .ok()
    }
}
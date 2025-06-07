use anyhow::{Context as _, Result};
use filetime::{FileTime, set_file_mtime};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::{fs, io};

const STORE_DIR: &str = "hash_store";
const STORE_EXPIRY: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub async fn get_or_store(
    key: impl Into<&[u8]>,
    extension: impl Into<&str>,
    producer: impl Future<Output = Result<Vec<u8>>>,
) -> Result<PathBuf> {
    let store = Path::new(STORE_DIR);
    create_dir(store).await?;

    let mut path = store.join(sha256(key.into()));
    path.set_extension(extension.into());

    if path.exists() {
        set_file_mtime(&path, FileTime::now())?;
    } else {
        fs::write(&path, producer.await?).await?;
    }

    Ok(path)
}

pub async fn purge_expired() -> Result<()> {
    create_dir(Path::new(STORE_DIR)).await?;
    let mut dir = fs::read_dir(STORE_DIR).await?;
    while let Some(entry) = dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        if metadata.is_file() && !metadata.modified()?.elapsed().is_ok_and(|d| d < STORE_EXPIRY) {
            let path = entry.path();
            tracing::info!("Deleting old file: {path:?}");
            fs::remove_file(path).await?;
        }
    }
    Ok(())
}

async fn create_dir(path: &Path) -> Result<()> {
    if let Err(e) = fs::create_dir_all(path).await
        && e.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(e).context("Failed to create directory");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hex::encode(&hasher.finalize()[..])
}

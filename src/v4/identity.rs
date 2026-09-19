use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use tauri::{AppHandle, Manager};

const ID_PREFIX: &str = "ab-install-";

pub(crate) async fn device_id(app: &AppHandle) -> Result<String> {
    let directory = app.path().app_local_data_dir()?.join("host-identity");
    tokio::task::spawn_blocking(move || load_or_create(&directory))
        .await
        .context("host identity task failed")?
}

fn read_id(path: &Path) -> Result<String> {
    let value = fs::read_to_string(path).context("read host identity")?;
    ensure!(
        value.strip_prefix(ID_PREFIX).is_some_and(
            |suffix| suffix.len() == 32 && suffix.bytes().all(|b| b.is_ascii_hexdigit())
        ),
        "stored host identity is invalid"
    );
    Ok(value)
}

fn load_or_create(directory: &Path) -> Result<String> {
    fs::create_dir_all(directory).context("create host identity directory")?;
    let path = directory.join("id");
    match fs::metadata(&path) {
        Ok(_) => return read_id(&path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("inspect host identity"),
    }
    let id = format!("{ID_PREFIX}{}", hex::encode(rand::random::<[u8; 16]>()));
    let temporary = directory.join(format!(".{id}.tmp"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .context("create temporary host identity")?;
    let result = (|| {
        file.write_all(id.as_bytes())
            .context("write host identity")?;
        file.sync_all().context("persist host identity")?;
        // Publish a complete file without replacing a concurrent creator's identity.
        match fs::hard_link(&temporary, &path) {
            Ok(()) => Ok(id),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_id(&path),
            Err(error) => Err(error).context("publish host identity"),
        }
    })();
    drop(file);
    let _ = fs::remove_file(&temporary);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(std::path::PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "astrobox-id-test-{}",
                hex::encode(rand::random::<[u8; 16]>())
            )))
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn identity_is_persistent_and_installation_scoped() {
        let first = TestDirectory::new();
        let second = TestDirectory::new();
        let id = load_or_create(&first.0).unwrap();
        assert!(id.starts_with(ID_PREFIX));
        assert_eq!(id, load_or_create(&first.0).unwrap());
        assert_ne!(id, load_or_create(&second.0).unwrap());
    }

    #[test]
    fn concurrent_readers_converge_on_one_identity() {
        let directory = TestDirectory::new();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        let threads: Vec<_> = (0..12)
            .map(|_| {
                let path = directory.0.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    load_or_create(&path).unwrap()
                })
            })
            .collect();
        let ids: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect();
        assert!(ids.iter().all(|id| id == &ids[0]));
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn corrupt_identity_is_not_silently_replaced() {
        let directory = TestDirectory::new();
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(directory.0.join("id"), "broken").unwrap();
        assert!(load_or_create(&directory.0).is_err());
        assert_eq!(
            fs::read_to_string(directory.0.join("id")).unwrap(),
            "broken"
        );
    }
}

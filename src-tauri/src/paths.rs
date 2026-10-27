use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub fn app_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("OFFICE_GATEWAY_DATA_DIR") {
        return PathBuf::from(path);
    }
    dirs::data_dir()
        .expect("User application data directory is unavailable")
        .join("Claude Office Gateway")
}
pub fn config_path() -> PathBuf {
    app_dir().join("settings.json")
}
pub fn certificate_dir() -> PathBuf {
    app_dir().join("TLS")
}
pub fn certificate_path() -> PathBuf {
    certificate_dir().join("localhost.pem")
}
pub fn certificate_der_path() -> PathBuf {
    certificate_dir().join("localhost.cer")
}
pub fn key_path() -> PathBuf {
    certificate_dir().join("localhost-key.pem")
}

pub fn prepare_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Missing file parent"))?;
    prepare_dir(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    staged.write_all(data)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|error| error.error)?;
    Ok(())
}

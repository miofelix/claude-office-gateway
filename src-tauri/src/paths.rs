use anyhow::Result;
use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
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
pub fn trusted_marker_path() -> PathBuf {
    certificate_dir().join("trusted")
}
pub fn prepare_dir(path: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

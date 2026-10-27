use crate::paths;
use anyhow::{anyhow, Result};
use rcgen::generate_simple_self_signed;
use std::process::Command;

pub struct CertificateManager;

impl CertificateManager {
    pub fn ensure() -> Result<()> {
        paths::prepare_dir(&paths::certificate_dir())?;
        if paths::certificate_path().exists()
            && paths::key_path().exists()
            && paths::certificate_der_path().exists()
        {
            return Ok(());
        }
        let generated = generate_simple_self_signed(vec![
            "localhost".into(),
            "127.0.0.1".into(),
            "::1".into(),
        ])?;
        std::fs::write(paths::certificate_path(), generated.cert.pem())?;
        std::fs::write(paths::key_path(), generated.key_pair.serialize_pem())?;
        std::fs::write(paths::certificate_der_path(), generated.cert.der())?;
        Ok(())
    }

    pub fn exists() -> bool {
        paths::certificate_path().exists()
            && paths::key_path().exists()
            && paths::certificate_der_path().exists()
    }

    pub fn trusted() -> bool {
        if !Self::exists() || !paths::trusted_marker_path().exists() {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            return Command::new("/usr/bin/security")
                .args(["verify-cert", "-c"])
                .arg(paths::certificate_der_path())
                .args(["-p", "ssl", "-s", "localhost"])
                .output()
                .map(|value| value.status.success())
                .unwrap_or(false);
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }

    pub fn trust_current_user() -> Result<()> {
        Self::ensure()?;
        #[cfg(target_os = "macos")]
        {
            let keychain = dirs::home_dir()
                .ok_or_else(|| anyhow!("Cannot find the current user's home directory"))?
                .join("Library/Keychains/login.keychain-db");
            run(Command::new("/usr/bin/security")
                .args([
                    "add-trusted-cert",
                    "-r",
                    "trustRoot",
                    "-p",
                    "ssl",
                    "-s",
                    "localhost",
                    "-k",
                ])
                .arg(keychain)
                .arg(paths::certificate_der_path()))?;
        }
        #[cfg(target_os = "windows")]
        {
            run(Command::new("certutil.exe")
                .args(["-user", "-addstore", "Root"])
                .arg(paths::certificate_der_path()))?;
        }
        std::fs::write(paths::trusted_marker_path(), b"trusted")?;
        if Self::trusted() {
            Ok(())
        } else {
            Err(anyhow!("The localhost certificate is not trusted yet."))
        }
    }

    pub fn delete() -> Result<()> {
        if Self::exists() {
            #[cfg(target_os = "macos")]
            {
                let _ = Command::new("/usr/bin/security")
                    .args(["remove-trusted-cert"])
                    .arg(paths::certificate_der_path())
                    .output();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("certutil.exe")
                    .args([
                        "-user",
                        "-delstore",
                        "Root",
                        "Claude Office Gateway localhost",
                    ])
                    .output();
            }
        }
        let _ = std::fs::remove_dir_all(paths::certificate_dir());
        Ok(())
    }
}

fn run(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            Err(anyhow!("The local certificate operation failed."))
        } else {
            Err(anyhow!(detail))
        }
    }
}

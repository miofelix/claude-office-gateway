use crate::paths;
use anyhow::{anyhow, Result};
use rcgen::{CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose};
use std::process::Command;

pub const SUBJECT: &str = "Claude Office Gateway localhost";
pub struct CertificateManager;
impl CertificateManager {
    pub fn exists() -> bool {
        paths::certificate_dir().exists()
            && [
                paths::certificate_path(),
                paths::certificate_der_path(),
                paths::key_path(),
                paths::certificate_dir().join("localhost.p12"),
            ]
            .iter()
            .any(|p| p.exists())
    }
    pub fn ensure() -> Result<()> {
        paths::prepare_dir(&paths::certificate_dir())?;
        if paths::certificate_path().exists() && paths::certificate_der_path().exists() {
            if !paths::key_path().exists() {
                let legacy = paths::certificate_dir().join("localhost.p12");
                if !legacy.exists() {
                    return Err(anyhow!("The local certificate identity is incomplete. Remove it in settings and install a new certificate."));
                }
                #[cfg(target_os = "macos")]
                {
                    let output = Command::new("/usr/bin/openssl")
                        .args(["pkcs12", "-in"])
                        .arg(legacy)
                        .args([
                            "-nocerts",
                            "-nodes",
                            "-passin",
                            "pass:office-gateway-local-identity",
                        ])
                        .output()?;
                    if !output.status.success() {
                        return Err(anyhow!("Could not migrate the saved certificate identity. The original files were preserved."));
                    }
                    paths::write_private(&paths::key_path(), &output.stdout)?;
                }
            }
            return Ok(());
        }
        if Self::exists() {
            return Err(anyhow!("The local certificate is incomplete. Remove it in settings and install a new certificate."));
        }
        let (pem, key, der) = generate_identity()?;
        paths::write_private(&paths::certificate_path(), pem.as_bytes())?;
        paths::write_private(&paths::key_path(), key.as_bytes())?;
        paths::write_private(&paths::certificate_der_path(), &der)?;
        Ok(())
    }
    pub fn trusted() -> bool {
        if !paths::certificate_der_path().exists() {
            return false;
        }
        #[cfg(target_os = "macos")]
        {
            Command::new("/usr/bin/security")
                .args(["verify-cert", "-c"])
                .arg(paths::certificate_der_path())
                .args(["-p", "ssl", "-s", "localhost", "-L"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(target_os = "windows")]
        {
            let path = paths::certificate_der_path()
                .to_string_lossy()
                .replace('\'', "''");
            let script=format!("$ErrorActionPreference='Stop'; $c=[System.Security.Cryptography.X509Certificates.X509Certificate2]::new('{path}'); $stored=Get-Item ('Cert:\\CurrentUser\\Root\\'+$c.Thumbprint) -ErrorAction SilentlyContinue; if ($null -eq $stored -or $c.NotAfter -lt (Get-Date) -or $c.NotBefore -gt (Get-Date)) {{exit 1}}; exit 0");
            powershell(&script)
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }
    pub fn trust_current_user() -> Result<()> {
        Self::ensure()?;
        #[cfg(target_os = "macos")]
        {
            let keychain = dirs::home_dir()
                .ok_or_else(|| anyhow!("Cannot find login Keychain"))?
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
        if Self::trusted() {
            Ok(())
        } else {
            Err(anyhow!(
                "Complete the system certificate confirmation, then refresh the connection page."
            ))
        }
    }
    pub fn delete() -> Result<()> {
        if !Self::exists() {
            return Ok(());
        }
        let encoded=std::fs::read(paths::certificate_der_path()).map_err(|_|anyhow!("The certificate file is missing. Remove this app's certificate trust using the system certificate manager before deleting its identity."))?;
        #[cfg(target_os = "macos")]
        {
            use security_framework::trust_settings::{Domain, TrustSettings};
            let settings = TrustSettings::new(Domain::User);
            let has_trust = settings.iter()?.any(|cert| cert.to_der() == encoded);
            if has_trust {
                run(Command::new("/usr/bin/security")
                    .arg("remove-trusted-cert")
                    .arg(paths::certificate_der_path()))?;
            }
            let certificate = security_framework::certificate::SecCertificate::from_der(&encoded)?;
            if let Err(error) = certificate.delete() {
                if error.code() != -25300 {
                    return Err(error.into());
                }
            }
        }
        #[cfg(target_os = "windows")]
        {
            use sha1::{Digest, Sha1};
            let thumbprint = format!("{:X}", Sha1::digest(&encoded));
            run(Command::new("certutil.exe").args(["-user", "-delstore", "Root", &thumbprint]))?;
        }
        for name in [
            "localhost.pem",
            "localhost.cer",
            "localhost-key.pem",
            "localhost.p12",
            "trusted",
        ] {
            let path = paths::certificate_dir().join(name);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}

pub fn generate_identity() -> Result<(String, String, Vec<u8>)> {
    let mut params =
        CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into(), "::1".into()])?;
    params.distinguished_name.push(DnType::CommonName, SUBJECT);
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.not_before = time::OffsetDateTime::now_utc() - time::Duration::hours(1);
    params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(365);
    let key = KeyPair::generate()?;
    let cert = params.self_signed(&key)?;
    Ok((cert.pem(), key.serialize_pem(), cert.der().to_vec()))
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!("The system certificate operation was cancelled or failed ({}). Identity files were preserved.",output.status))
    }
}
#[cfg(target_os = "windows")]
fn powershell(script: &str) -> std::io::Result<std::process::Output> {
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
}

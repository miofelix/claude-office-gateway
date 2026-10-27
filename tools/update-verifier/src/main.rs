use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use std::{env, fs};
fn verify(config: &str, signature: &str, payload: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config: serde_json::Value = serde_json::from_slice(&fs::read(config)?)?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("Missing updater public key")?;
    let public = String::from_utf8(STANDARD.decode(key.trim())?)?;
    let signature = String::from_utf8(STANDARD.decode(fs::read_to_string(signature)?.trim())?)?;
    let public = PublicKey::decode(&public)?;
    let signature = Signature::decode(&signature)?;
    public.verify(&fs::read(payload)?, &signature, false)?;
    Ok(())
}
fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: office-update-verifier <tauri.conf.json> <payload.sig> <payload>");
        std::process::exit(2);
    }
    match verify(&args[1], &args[2], &args[3]) {
        Ok(()) => println!("Update signature verified"),
        Err(error) => {
            eprintln!("Update signature rejected: {error}");
            std::process::exit(1);
        }
    }
}

use anyhow::{anyhow, Result};
use keyring::Entry;
use uuid::Uuid;

const SERVICE: &str = "com.miofelix.office-gateway";
const UPSTREAM_ACCOUNT: &str = "upstream-api-key";
const GATEWAY_ACCOUNT: &str = "gateway-api-key";

fn upstream_account(provider_id: &str) -> String {
    if provider_id == "default" {
        UPSTREAM_ACCOUNT.into()
    } else {
        format!("upstream-api-key-{provider_id}")
    }
}

pub fn validate(value: &str) -> Result<()> {
    if !value.is_empty()
        && value.len() <= 4096
        && value
            .chars()
            .all(|character| character.is_ascii() && (33..=126).contains(&(character as u8)))
    {
        Ok(())
    } else {
        Err(anyhow!(
            "Enter a key without spaces, line breaks, or non-ASCII characters."
        ))
    }
}

fn entry(account: &str) -> Result<Entry> {
    Ok(Entry::new(SERVICE, account)?)
}

pub fn read_upstream_key(provider_id: &str) -> Result<Option<String>> {
    match entry(&upstream_account(provider_id))?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn write_upstream_key(provider_id: &str, value: &str) -> Result<()> {
    validate(value)?;
    entry(&upstream_account(provider_id))?.set_password(value)?;
    Ok(())
}

pub fn gateway_token() -> Result<String> {
    match entry(GATEWAY_ACCOUNT)?.get_password() {
        Ok(value) if !value.is_empty() => Ok(value),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let token = format!("og_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
            entry(GATEWAY_ACCOUNT)?.set_password(&token)?;
            Ok(token)
        }
        Err(error) => Err(error.into()),
    }
}

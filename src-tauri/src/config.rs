use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamProtocol {
    Anthropic,
    #[default]
    OpenaiChat,
    OpenaiResponses,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    #[serde(alias = "baseURL")]
    pub base_url: String,
    #[serde(default)]
    #[serde(alias = "upstreamProtocol")]
    pub upstream_protocol: UpstreamProtocol,
    #[serde(default)]
    #[serde(alias = "upstreamModel")]
    pub upstream_model: String,
    #[serde(default = "default_public_model")]
    #[serde(alias = "publicModel")]
    pub public_model: String,
    #[serde(default = "default_display_name")]
    #[serde(alias = "displayName")]
    pub display_name: String,
    #[serde(default = "default_auth_header")]
    #[serde(alias = "authHeader")]
    pub auth_header: String,
    #[serde(default = "default_max_tokens_field")]
    #[serde(alias = "maxTokensField")]
    pub max_tokens_field: String,
    #[serde(default = "default_timeout")]
    #[serde(alias = "timeoutSeconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_allowed_origins")]
    #[serde(alias = "allowedOrigins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_probe_cache", alias = "probeCacheSeconds")]
    pub probe_cache_seconds: u64,
    #[serde(default = "default_request_limit", alias = "maxRequestBytes")]
    pub max_request_bytes: usize,
}

fn default_probe_cache() -> u64 {
    300
}
fn default_request_limit() -> usize {
    32 * 1024 * 1024
}

fn default_port() -> u16 {
    15724
}
fn default_public_model() -> String {
    "claude-office-custom".into()
}
fn default_display_name() -> String {
    "Custom model".into()
}
fn default_auth_header() -> String {
    "authorization".into()
}
fn default_max_tokens_field() -> String {
    "max_completion_tokens".into()
}
fn default_timeout() -> u64 {
    120
}
fn default_allowed_origins() -> Vec<String> {
    vec!["https://pivot.claude.ai".into()]
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            probe_cache_seconds: default_probe_cache(),
            max_request_bytes: default_request_limit(),
            port: default_port(),
            base_url: String::new(),
            upstream_protocol: UpstreamProtocol::default(),
            upstream_model: String::new(),
            public_model: default_public_model(),
            display_name: default_display_name(),
            auth_header: default_auth_header(),
            max_tokens_field: default_max_tokens_field(),
            timeout_seconds: default_timeout(),
            allowed_origins: default_allowed_origins(),
        }
    }
}

impl GatewayConfig {
    pub fn validate(&self) -> Result<()> {
        if !(1024..=65535).contains(&self.port) {
            return Err(anyhow!("Choose a local port between 1024 and 65535."));
        }
        let parsed = Url::parse(self.base_url.trim())
            .map_err(|_| anyhow!("Enter an HTTPS API base URL."))?;
        if parsed.scheme() != "https"
            || parsed.host_str().unwrap_or_default().is_empty()
            || parsed.username() != ""
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(anyhow!(
                "Enter an HTTPS API base URL without credentials, query parameters, or fragments."
            ));
        }
        if self.upstream_model.trim().is_empty() || self.display_name.trim().is_empty() {
            return Err(anyhow!("Enter the provider model and display name."));
        }
        if self.public_model.len() > 128
            || self.public_model.is_empty()
            || !self
                .public_model
                .chars()
                .enumerate()
                .all(|(index, character)| {
                    if index == 0 {
                        character.is_ascii_alphanumeric()
                    } else {
                        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
                    }
                })
        {
            return Err(anyhow!(
                "Office model ID must start with a letter or number and contain only letters, numbers, dots, underscores, or hyphens."
            ));
        }
        if !matches!(self.auth_header.as_str(), "authorization" | "x-api-key") {
            return Err(anyhow!("Provider authentication is invalid."));
        }
        if !matches!(
            self.max_tokens_field.as_str(),
            "max_tokens" | "max_completion_tokens"
        ) {
            return Err(anyhow!("The provider token limit field is invalid."));
        }
        if !(1..=600).contains(&self.timeout_seconds) {
            return Err(anyhow!(
                "Provider timeout must be between 1 and 600 seconds."
            ));
        }
        if self.probe_cache_seconds > 3600
            || !(1024..=32 * 1024 * 1024).contains(&self.max_request_bytes)
        {
            return Err(anyhow!("Gateway request or cache limit is invalid."));
        }
        if self.allowed_origins.is_empty()
            || self.allowed_origins.iter().any(|origin| {
                let Ok(url) = Url::parse(origin) else {
                    return true;
                };
                url.scheme() != "https"
                    || url.host_str().is_none()
                    || url.username() != ""
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || !matches!(url.path(), "" | "/")
            })
        {
            return Err(anyhow!("Allowed Office origins must be HTTPS origins."));
        }
        Ok(())
    }

    pub fn endpoint(&self, resource: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{base}/{resource}")
        } else {
            format!("{base}/v1/{resource}")
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let config: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            let old = std::fs::read(path)?;
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&old) {
                let backup = path.with_extension("swift-backup.json");
                if value.get("baseURL").is_some() && !backup.exists() {
                    crate::paths::write_private(&backup, &old)?;
                }
            }
        }
        crate::paths::write_private(path, &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> GatewayConfig {
        GatewayConfig {
            base_url: "https://api.example.com".into(),
            upstream_model: "model".into(),
            ..Default::default()
        }
    }

    #[test]
    fn validates_a_provider_configuration() {
        assert!(valid().validate().is_ok());
        assert_eq!(
            valid().endpoint("messages"),
            "https://api.example.com/v1/messages"
        );
    }

    #[test]
    fn rejects_insecure_or_credentialed_urls() {
        for value in [
            "http://api.example.com",
            "https://user:pass@api.example.com",
            "https://api.example.com?key=secret",
        ] {
            let mut config = valid();
            config.base_url = value.into();
            assert!(config.validate().is_err());
        }
    }
}

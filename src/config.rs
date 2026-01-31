use serde::Deserialize;
use std::fs;

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub discord: Discord,
    pub roles: Roles,
    pub pokemon: Pokemon,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Discord {
    pub token: String,
    pub guild_id: u64,

    #[serde(default = "default_captcha_perm")]
    pub captcha_default_permission: String,
}

fn default_captcha_perm() -> String {
    "MANAGE_GUILD".to_string()
}


#[derive(Debug, Clone, Deserialize)]
pub struct Roles {
    pub verify: u64,
    pub staff: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pokemon {
    pub language: String,
}

impl Config {
    pub fn load() -> Result<Self, AnyError> {
        let text = fs::read_to_string("config.toml")?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }
}

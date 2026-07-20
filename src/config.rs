use poise::serenity_prelude as serenity;
use serde::Deserialize;
use std::{fs, time::Duration};

pub type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub guild: Guild,
    pub verify: Verify,
    pub pokemon: Pokemon,
    pub greeter: Greeter,
    #[serde(default)]
    pub database: Database,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Database {
    #[serde(default = "default_database_path")]
    pub path: String,
}

impl Default for Database {
    fn default() -> Self {
        Self {
            path: default_database_path(),
        }
    }
}

fn default_database_path() -> String {
    crate::db::DEFAULT_DATABASE_PATH.to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Guild {
    pub guild_id: serenity::GuildId,
    pub staff_role_id: serenity::RoleId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verify {
    pub captcha_default_permission: String,
    pub verify_role_id: serenity::RoleId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Greeter {
    pub channel_id: serenity::ChannelId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pokemon {
    pub max_retry: usize,
    #[serde(with = "humantime_serde")]
    pub time_limit: Duration,
}

impl Config {
    pub fn load() -> Result<Self, AnyError> {
        let text = fs::read_to_string("config.toml")?;
        let cfg: Config = toml::from_str(&text)?;
        Ok(cfg)
    }
}

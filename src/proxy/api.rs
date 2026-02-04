use crate::{Error, http};
use reqwest::multipart;
use serde::Deserialize;
use std::ops::Deref;

const PROXYSCRAPE_GET_PROXY_ENDPOINT: &str =
    "https://api.proxyscrape.com/?request=displayproxies&proxytype=all&timeout=1500";

const PROXYSCRAPE_CHECK_PROXY_ENDPOINT: &str = "https://api.proxyscrape.com/v2/online_check.php";

#[derive(Deserialize)]
#[serde(untagged)]
pub enum OptString {
    Str(String),
    #[allow(dead_code)]
    Bool(bool),
}

#[derive(Deserialize)]
pub struct ProxyCheckResult {
    pub working: bool,
    pub r#type: OptString,
    pub ip: String,
    pub port: String,
    pub country: OptString,
    #[allow(dead_code)]
    pub ind: String,
}

#[derive(Deserialize)]
pub struct ProxyCheckResults(Vec<ProxyCheckResult>);

impl Deref for ProxyCheckResults {
    type Target = Vec<ProxyCheckResult>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct Proxy {
    pub ip: String,
    pub port: String,
}

pub async fn get_proxies() -> reqwest::Result<Vec<Proxy>> {
    Ok(http::CLIENT
        .get(PROXYSCRAPE_GET_PROXY_ENDPOINT)
        .send()
        .await?
        .text()
        .await?
        .lines()
        .filter_map(|l| {
            l.split_once(":").and_then(|p| {
                if !(p.0.is_empty() || p.1.is_empty()) {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .map(|(ip, port)| Proxy {
            ip: ip.to_string(),
            port: port.to_string(),
        })
        .collect())
}

pub async fn check_proxies(proxies: &[Proxy]) -> Result<ProxyCheckResults, Error> {
    let mut form = multipart::Form::new();

    for (i, proxy) in proxies.iter().enumerate() {
        form = form.text("ip_addr[]", format!("{}:{}-{}", proxy.ip, proxy.port, i));
    }

    Ok(http::CLIENT
        .post(PROXYSCRAPE_CHECK_PROXY_ENDPOINT)
        .multipart(form)
        .send()
        .await?
        .json()
        .await?)
}

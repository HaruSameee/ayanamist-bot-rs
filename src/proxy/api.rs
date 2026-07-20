use crate::{Error, http};
use reqwest::multipart;
use serde::Deserialize;
use std::ops::Deref;

const PROXYSCRAPE_BASE_URL: &str = "https://api.proxyscrape.com";
const PROXYSCRAPE_GET_PROXY_PATH: &str = "/?request=displayproxies&proxytype=all&timeout=1500";
const PROXYSCRAPE_CHECK_PROXY_PATH: &str = "/v2/online_check.php";

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

pub struct ProxyscrapeClient {
    base_url: String,
}

impl ProxyscrapeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    pub async fn get_proxies(&self) -> reqwest::Result<Vec<Proxy>> {
        Ok(http::CLIENT
            .get(format!("{}{}", self.base_url, PROXYSCRAPE_GET_PROXY_PATH))
            .send()
            .await?
            .text()
            .await?
            .lines()
            .filter_map(|l| {
                l.split_once(":")
                    .filter(|p| !(p.0.is_empty() || p.1.is_empty()))
            })
            .map(|(ip, port)| Proxy {
                ip: ip.to_string(),
                port: port.to_string(),
            })
            .collect())
    }

    pub async fn check_proxies(&self, proxies: &[Proxy]) -> Result<ProxyCheckResults, Error> {
        let mut form = multipart::Form::new();

        for (i, proxy) in proxies.iter().enumerate() {
            form = form.text("ip_addr[]", format!("{}:{}-{}", proxy.ip, proxy.port, i));
        }

        Ok(http::CLIENT
            .post(format!("{}{}", self.base_url, PROXYSCRAPE_CHECK_PROXY_PATH))
            .multipart(form)
            .send()
            .await?
            .json()
            .await?)
    }
}

impl Default for ProxyscrapeClient {
    fn default() -> Self {
        Self::new(PROXYSCRAPE_BASE_URL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn proxy(ip: &str, port: &str) -> Proxy {
        Proxy {
            ip: ip.to_string(),
            port: port.to_string(),
        }
    }

    #[tokio::test]
    async fn get_proxies_parses_lines_and_skips_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .and(query_param("request", "displayproxies"))
            .and(query_param("proxytype", "all"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(
                    "1.2.3.4:8080\n\nno colon line\n:8080\n1.2.3.4:\n5.6.7.8:1080\n",
                ),
            )
            .mount(&server)
            .await;

        let proxies = ProxyscrapeClient::new(server.uri())
            .get_proxies()
            .await
            .unwrap();

        assert_eq!(proxies.len(), 2);
        assert_eq!(proxies[0].ip, "1.2.3.4");
        assert_eq!(proxies[0].port, "8080");
        assert_eq!(proxies[1].ip, "5.6.7.8");
        assert_eq!(proxies[1].port, "1080");
    }

    #[tokio::test]
    async fn check_proxies_sends_multipart_form_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/online_check.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let proxies = vec![proxy("1.2.3.4", "8080"), proxy("5.6.7.8", "1080")];
        ProxyscrapeClient::new(server.uri())
            .check_proxies(&proxies)
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = String::from_utf8(requests[0].body.clone()).unwrap();
        assert_eq!(body.matches("name=\"ip_addr[]\"").count(), 2);
        assert!(body.contains("1.2.3.4:8080-0"));
        assert!(body.contains("5.6.7.8:1080-1"));
    }

    #[tokio::test]
    async fn check_proxies_parses_json_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/online_check.php"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "working": true,
                    "type": "http",
                    "ip": "1.2.3.4",
                    "port": "8080",
                    "country": "jp",
                    "ind": "x"
                },
                {
                    "working": false,
                    "type": false,
                    "ip": "5.6.7.8",
                    "port": "1080",
                    "country": false,
                    "ind": "y"
                }
            ])))
            .mount(&server)
            .await;

        let results = ProxyscrapeClient::new(server.uri())
            .check_proxies(&[proxy("1.2.3.4", "8080")])
            .await
            .unwrap();

        assert_eq!(results.len(), 2);

        assert!(results[0].working);
        assert_eq!(results[0].ip, "1.2.3.4");
        assert_eq!(results[0].port, "8080");
        match &results[0].r#type {
            OptString::Str(s) => assert_eq!(s, "http"),
            OptString::Bool(_) => panic!("type should be a string"),
        }
        match &results[0].country {
            OptString::Str(s) => assert_eq!(s, "jp"),
            OptString::Bool(_) => panic!("country should be a string"),
        }

        assert!(!results[1].working);
        match &results[1].r#type {
            OptString::Bool(b) => assert!(!b),
            OptString::Str(_) => panic!("type should be a bool"),
        }
        match &results[1].country {
            OptString::Bool(b) => assert!(!b),
            OptString::Str(_) => panic!("country should be a bool"),
        }
    }
}

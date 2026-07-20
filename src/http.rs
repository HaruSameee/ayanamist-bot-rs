use reqwest::Client;
use std::sync::LazyLock;
use std::time::Duration;

/// 外部 API リクエストの全体タイムアウト。
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// タイムアウトを設定した HTTP クライアントを構築する。
pub fn build_client(timeout: Duration) -> reqwest::Result<Client> {
    Client::builder().timeout(timeout).build()
}

pub static CLIENT: LazyLock<Client> =
    LazyLock::new(|| build_client(REQUEST_TIMEOUT).unwrap_or_else(|_| Client::new()));

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn client_times_out_on_slow_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&server)
            .await;

        let client = build_client(Duration::from_millis(100)).unwrap();
        let result = client.get(format!("{}/slow", server.uri())).send().await;

        let err = result.expect_err("slow response should time out");
        assert!(err.is_timeout());
    }
}

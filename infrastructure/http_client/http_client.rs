use rootcause::Result;
use std::fmt::Debug;
use std::ops::Deref;
use std::time::Duration;

use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;

/// 底层 `reqwest::Client` 的选项（middleware 之前应用）。
#[derive(Clone)]
pub struct HttpClientConfig {
    pub request_timeout: Duration,
    pub connect_timeout: Duration,
    pub max_retries: u32,
    pub user_agent: &'static str,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_retries: 3,
            user_agent: concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")),
        }
    }
}

#[derive(Clone)]
pub struct HttpClient {
    client: ClientWithMiddleware,
}

impl HttpClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_with_config(config: HttpClientConfig) -> Result<Self> {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(config.max_retries);

        let client = Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(config.connect_timeout)
            .http2_adaptive_window(true)
            .use_rustls_tls()
            .user_agent(config.user_agent)
            .build()?;

        let retry = RetryTransientMiddleware::new_with_policy(retry_policy);

        let client = ClientBuilder::new(client).with(retry).build();

        Ok(Self { client })
    }
}

impl Default for HttpClient {
    #[allow(clippy::expect_used, reason = "default HttpClientConfig never fails")]
    fn default() -> Self {
        Self::try_with_config(HttpClientConfig::default()).expect("default HttpClientConfig")
    }
}

impl Deref for HttpClient {
    type Target = ClientWithMiddleware;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpClient").finish()
    }
}

#[cfg(test)]
mod tests {
    //! 依赖公网 **httpbin.org**；不可达时跳过。不依赖 tracing 订阅器。

    use super::*;

    const HTTPBIN_HTTP: &str = "http://httpbin.org";
    const HTTPBIN_HTTPS: &str = "https://httpbin.org";

    async fn httpbin_unavailable() -> bool {
        let Ok(probe) = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(3))
            .build()
        else {
            return true;
        };
        match tokio::time::timeout(
            Duration::from_secs(8),
            probe.get(format!("{HTTPBIN_HTTPS}/get")).send(),
        )
        .await
        {
            Ok(Ok(r)) => !r.status().is_success(),
            _ => true,
        }
    }

    #[tokio::test]
    async fn httpbin_https_get_ok() {
        if httpbin_unavailable().await {
            eprintln!("skip httpbin_https_get_ok: httpbin unreachable");
            return;
        }

        let client = HttpClient::try_with_config(HttpClientConfig::default()).expect("client");
        let res = client
            .get(format!("{HTTPBIN_HTTPS}/get"))
            .send()
            .await
            .expect("send");
        assert!(res.status().is_success(), "status {}", res.status());
    }

    #[tokio::test]
    async fn httpbin_http_get_ok() {
        if httpbin_unavailable().await {
            eprintln!("skip httpbin_http_get_ok: httpbin unreachable");
            return;
        }

        let client = HttpClient::try_with_config(HttpClientConfig::default()).expect("client");
        let res = client
            .get(format!("{HTTPBIN_HTTP}/get"))
            .send()
            .await
            .expect("send");
        assert!(res.status().is_success(), "status {}", res.status());
    }

    /// 5xx 会触发重试，最终仍返回带 500 的 [`reqwest::Response`]（不 `error_for_status`）。
    #[tokio::test]
    async fn httpbin_https_status_500() {
        if httpbin_unavailable().await {
            eprintln!("skip httpbin_https_status_500: httpbin unreachable");
            return;
        }

        let mut cfg = HttpClientConfig::default();
        cfg.max_retries = 2;
        cfg.request_timeout = Duration::from_secs(20);

        let client = HttpClient::try_with_config(cfg).expect("client");
        let res = client
            .get(format!("{HTTPBIN_HTTPS}/status/500"))
            .send()
            .await
            .expect("send");

        assert_eq!(res.status(), reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// 短超时 + 长 delay：重试耗尽后应返回错误（覆盖重试路径，不依赖日志）。
    #[tokio::test]
    async fn httpbin_https_timeout_after_retries() {
        if httpbin_unavailable().await {
            eprintln!("skip httpbin_https_timeout_after_retries: httpbin unreachable");
            return;
        }

        let mut cfg = HttpClientConfig::default();
        cfg.max_retries = 2;
        cfg.request_timeout = Duration::from_millis(600);
        cfg.connect_timeout = Duration::from_secs(5);

        let client = HttpClient::try_with_config(cfg).expect("client");
        let err = client
            .get(format!("{HTTPBIN_HTTPS}/delay/3"))
            .send()
            .await
            .expect_err("expected failure after timeouts/retries");

        match err {
            reqwest_middleware::Error::Reqwest(e) => assert!(e.is_timeout(), "{e:?}"),
            reqwest_middleware::Error::Middleware(m) => {
                let msg = format!("{m:#}");
                assert!(
                    msg.contains("timed out") || msg.contains("timeout") || msg.contains("Timeout"),
                    "{msg}"
                );
            }
        }
    }
}

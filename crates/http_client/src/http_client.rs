pub mod github;
pub mod proxy;

pub use anyhow::{anyhow, Result};
use futures::future::BoxFuture;
use futures_lite::FutureExt;
use isahc::{
    config::{Configurable, RedirectPolicy},
    RequestExt,
};
pub use isahc::{
    http::{self, Method, StatusCode, Uri},
    AsyncBody, Error, HttpClient as IsahcHttpClient, Request, Response,
};
use parking_lot::Mutex;
use proxy::Proxy;
#[cfg(feature = "test-support")]
use std::fmt;
use std::{sync::Arc, time::Duration};
pub use url::Url;

pub trait HttpClient: Send + Sync {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>>;

    fn get<'a>(
        &'a self,
        uri: &str,
        body: AsyncBody,
        follow_redirects: bool,
    ) -> BoxFuture<'a, Result<Response<AsyncBody>, Error>> {
        let request = isahc::Request::builder()
            .redirect_policy(if follow_redirects {
                RedirectPolicy::Follow
            } else {
                RedirectPolicy::None
            })
            .method(Method::GET)
            .uri(uri)
            .body(body);
        match request {
            Ok(request) => self.send(request),
            Err(error) => async move { Err(error.into()) }.boxed(),
        }
    }

    fn post_json<'a>(
        &'a self,
        uri: &str,
        body: AsyncBody,
    ) -> BoxFuture<'a, Result<Response<AsyncBody>, Error>> {
        let request = isahc::Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("Content-Type", "application/json")
            .body(body);
        match request {
            Ok(request) => self.send(request),
            Err(error) => async move { Err(error.into()) }.boxed(),
        }
    }

    fn proxy(&self) -> Proxy;
}

/// An [`HttpClient`] that may have a proxy.
pub struct HttpClientWithProxy {
    client: Arc<dyn HttpClient>,
    proxy: Proxy,
}

impl HttpClientWithProxy {
    /// Returns a new [`HttpClientWithProxy`] with the given proxy URL.
    pub fn new(proxy: Proxy, user_agent: Option<String>) -> Self {
        let mut builder = isahc::HttpClient::builder()
            .connect_timeout(Duration::from_secs(5))
            .low_speed_timeout(100, Duration::from_secs(5));
        if let Some(user_agent) = user_agent {
            builder = builder.default_header("User-Agent", user_agent);
        }

        Self {
            client: Arc::new(builder.build().unwrap()),
            proxy,
        }
    }
}

impl HttpClient for HttpClientWithProxy {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        // add proxy here
        match req
            .to_builder()
            .proxy(self.proxy().to_uri())
            .body(req.into_body())
        {
            Ok(request) => self.client.send(request),
            Err(error) => async move { Err(error.into()) }.boxed(),
        }
    }

    fn proxy(&self) -> Proxy {
        self.proxy.clone()
    }
}

impl HttpClient for Arc<HttpClientWithProxy> {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        self.client.send(req)
    }

    fn proxy(&self) -> Proxy {
        self.proxy.clone()
    }
}

/// An [`HttpClient`] that has a base URL.
pub struct HttpClientWithUrl {
    base_url: Mutex<String>,
    client: HttpClientWithProxy,
}

impl HttpClientWithUrl {
    /// Returns a new [`HttpClientWithUrl`] with the given base URL.
    pub fn new(base_url: impl Into<String>, proxy: Proxy, user_agent: Option<String>,) -> Self {
        let client = HttpClientWithProxy::new(proxy, user_agent);
        Self {
            base_url: Mutex::new(base_url.into()),
            client,
        }
    }

    /// Returns the base URL.
    pub fn base_url(&self) -> String {
        self.base_url.lock().clone()
    }

    /// Sets the base URL.
    pub fn set_base_url(&self, base_url: impl Into<String>) {
        let base_url = base_url.into();
        *self.base_url.lock() = base_url;
    }

    /// Builds a URL using the given path.
    pub fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }

    /// Builds a Zed API URL using the given path.
    pub fn build_zed_api_url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url> {
        let base_url = self.base_url();
        let base_api_url = match base_url.as_ref() {
            "https://zed.dev" => "https://api.zed.dev",
            "https://staging.zed.dev" => "https://api-staging.zed.dev",
            "http://localhost:3000" => "http://localhost:8080",
            other => other,
        };

        Ok(Url::parse_with_params(
            &format!("{}{}", base_api_url, path),
            query,
        )?)
    }

    /// Builds a Zed LLM URL using the given path.
    pub fn build_zed_llm_url(&self, path: &str, query: &[(&str, &str)]) -> Result<Url> {
        let base_url = self.base_url();
        let base_api_url = match base_url.as_ref() {
            "https://zed.dev" => "https://llm.zed.dev",
            "https://staging.zed.dev" => "https://llm-staging.zed.dev",
            "http://localhost:3000" => "http://localhost:8080",
            other => other,
        };

        Ok(Url::parse_with_params(
            &format!("{}{}", base_api_url, path),
            query,
        )?)
    }
}

impl HttpClient for Arc<HttpClientWithUrl> {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        self.client.send(req)
    }

    fn proxy(&self) -> Proxy {
        self.client.proxy()
    }
}

impl HttpClient for HttpClientWithUrl {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        self.client.send(req)
    }

    fn proxy(&self) -> Proxy {
        self.client.proxy()
    }
}

pub fn client(proxy: Proxy, user_agent: Option<String>) -> Arc<dyn HttpClient> {
    Arc::new(HttpClientWithProxy::new(proxy, user_agent))
}

impl HttpClient for isahc::HttpClient {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        let client = self.clone();
        Box::pin(async move { client.send_async(req).await })
    }

    fn proxy(&self) -> Proxy {
        Proxy::no_proxy()
    }
}

#[cfg(feature = "test-support")]
type FakeHttpHandler = Box<
    dyn Fn(Request<AsyncBody>) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>>
        + Send
        + Sync
        + 'static,
>;

#[cfg(feature = "test-support")]
pub struct FakeHttpClient {
    handler: FakeHttpHandler,
}

#[cfg(feature = "test-support")]
impl FakeHttpClient {
    pub fn create<Fut, F>(handler: F) -> Arc<HttpClientWithUrl>
    where
        Fut: futures::Future<Output = Result<Response<AsyncBody>, Error>> + Send + 'static,
        F: Fn(Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        Arc::new(HttpClientWithUrl {
            base_url: Mutex::new("http://test.example".into()),
            client: HttpClientWithProxy {
                client: Arc::new(Self {
                    handler: Box::new(move |req| Box::pin(handler(req))),
                }),
                proxy: Proxy::no_proxy(),
            },
        })
    }

    pub fn with_404_response() -> Arc<HttpClientWithUrl> {
        Self::create(|_| async move {
            Ok(Response::builder()
                .status(404)
                .body(Default::default())
                .unwrap())
        })
    }

    pub fn with_200_response() -> Arc<HttpClientWithUrl> {
        Self::create(|_| async move {
            Ok(Response::builder()
                .status(200)
                .body(Default::default())
                .unwrap())
        })
    }
}

#[cfg(feature = "test-support")]
impl fmt::Debug for FakeHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeHttpClient").finish()
    }
}

#[cfg(feature = "test-support")]
impl HttpClient for FakeHttpClient {
    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>, Error>> {
        let future = (self.handler)(req);
        Box::pin(async move { future.await.map(Into::into) })
    }

    fn proxy(&self) -> Proxy {
        Proxy::no_proxy()
    }
}

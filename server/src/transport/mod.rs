pub mod hash;

pub mod lru;

use std::fmt::Debug;
use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_transport::{TransportError, TransportErrorKind, TransportResult};
use axum::async_trait;
use reqwest::header::HeaderMap;
use reqwest::Client;
use tracing::{debug, trace};
use url::Url;

#[derive(Clone, Debug)]
pub struct Http<T: Transport> {
    inner: T,
}

impl <T: Transport> Http<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

#[async_trait]
pub trait Transport : Send + Sync + Debug + Clone + 'static {
    async fn post(self, req: RequestPacket, headers: HeaderMap) -> TransportResult<ResponsePacket>;
}


/// An HTTP transport using `reqwest` referencing alloy-http-transport `Http<T>`
#[derive(Clone, Debug)]
pub struct DefaultTransport {
    client: Client,
    url: Url,
}

impl DefaultTransport {
    pub fn new(url: Url) -> Self {
        Self {
            client: Default::default(),
            url,
        }
    }
}
#[async_trait]
impl Transport for DefaultTransport {
    async fn post(self, req: RequestPacket, headers: HeaderMap) -> TransportResult<ResponsePacket> {
        let resp = self
            .client
            .post(self.url)
            .json(&req)
            .headers(headers)
            .send()
            .await
            .map_err(TransportErrorKind::custom)?;
        let status = resp.status();

        debug!(%status, "received response from server");

        let body = resp.bytes().await.map_err(TransportErrorKind::custom)?;

        trace!(body = %String::from_utf8_lossy(&body), "response body");

        if !status.is_success() {
            return Err(TransportErrorKind::http_error(
                status.as_u16(),
                String::from_utf8_lossy(&body).into_owned(),
            ));
        }

        serde_json::from_slice(&body)
            .map_err(|err| TransportError::deser_err(err, String::from_utf8_lossy(&body)))
    }
}

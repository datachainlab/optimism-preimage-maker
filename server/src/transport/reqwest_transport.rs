use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_primitives::{keccak256, B256};
use alloy_transport::{TransportError, TransportErrorKind, TransportFut, TransportResult};
use reqwest::Client;
use std::task;
use tower::Service;
use tracing::{debug, trace};
use url::Url;

use crate::transport::Http;

/// Http client with reqwest
impl Http<Client> {
    pub fn new(url: Url) -> Self {
        Self {
            client: Default::default(),
            url,
        }
    }

    async fn do_reqwest(self, req: RequestPacket) -> TransportResult<ResponsePacket> {
        let hash = from_request_to_hash(&req)
            .map_err(|err| TransportError::deser_err(err, "invalid request for hashing"))?;

        let mut headers = req.headers();
        headers.insert("X-Request-Hash", hash.to_string().parse().unwrap());

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

impl Service<RequestPacket> for Http<Client> {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let this = self.clone();
        Box::pin(this.do_reqwest(req))
    }
}

fn from_request_to_hash(req: &RequestPacket) -> Result<B256, serde_json::Error> {
    let body = match &req {
        RequestPacket::Single(r) => serde_json::to_vec(r)?,
        RequestPacket::Batch(_) => {
            let mut value = vec![];
            if let Some(batch) = req.as_batch() {
                for r in batch {
                    value.extend_from_slice(&r.params_hash().0);
                }
            }
            value
        }
    };
    Ok(keccak256(&body))
}

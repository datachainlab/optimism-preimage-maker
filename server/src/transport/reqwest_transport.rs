use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_primitives::{keccak256, B256};
use alloy_transport::{TransportError, TransportErrorKind, TransportFut, TransportResult};
use reqwest::Client;
use std::task;
use tower::Service;
use tracing::{debug, debug_span, info, trace, Instrument};
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
        let hash = from_request_to_hash(&req);
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

        // Unpack data from the response body. We do this regardless of
        // the status code, as we want to return the error in the body
        // if there is one.
        let body = resp.bytes().await.map_err(TransportErrorKind::custom)?;

        debug!(
            bytes = body.len(),
            "retrieved response body. Use `trace` for full body"
        );
        trace!(body = %String::from_utf8_lossy(&body), "response body");

        if !status.is_success() {
            return Err(TransportErrorKind::http_error(
                status.as_u16(),
                String::from_utf8_lossy(&body).into_owned(),
            ));
        }

        // Deserialize a Box<RawValue> from the body. If deserialization fails, return
        // the body as a string in the error. The conversion to String
        // is lossy and may not cover all the bytes in the body.
        serde_json::from_slice(&body)
            .map_err(|err| TransportError::deser_err(err, String::from_utf8_lossy(&body)))
    }
}

impl Service<RequestPacket> for Http<Client> {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    #[inline]
    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let this = self.clone();
        let span = debug_span!("ReqwestTransport", url = %this.url);
        Box::pin(this.do_reqwest(req).instrument(span))
    }
}

fn from_request_to_hash(req: &RequestPacket) -> B256 {
    match req {
        RequestPacket::Single(r) => r.params_hash(),
        RequestPacket::Batch(_) => {
            let mut value = vec![];
            if let Some(batch) = req.as_batch() {
                for r in batch {
                    value.extend_from_slice(&r.params_hash().0);
                }
            }
            keccak256(&value)
        }
    }
}

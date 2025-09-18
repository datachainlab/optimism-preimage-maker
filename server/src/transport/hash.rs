use alloy_json_rpc::{RequestPacket, ResponsePacket};
use alloy_primitives::{keccak256, B256};
use alloy_transport::{TransportError, TransportFut};
use std::task;
use tower::Service;

use crate::transport::Transport;

impl Service<RequestPacket> for Transport {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> task::Poll<Result<(), Self::Error>> {
        task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let hash = match from_request_to_hash(&req) {
            Ok(hash) => hash,
            Err(err) => {
                return Box::pin(async move {
                    Err(TransportError::deser_err(
                        err,
                        "invalid request for hashing",
                    ))
                });
            }
        };
        let mut headers = req.headers();
        headers.insert("X-Request-Hash", hash.to_string().parse().unwrap());

        let this = self.clone();
        Box::pin(this.post(req, headers))
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

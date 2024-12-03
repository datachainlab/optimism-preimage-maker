use os_pipe::{PipeReader, PipeWriter};
use alloy_primitives::{hex, Bytes};
use alloy_provider::ReqwestProvider;
use alloy_rpc_client::RpcClient;
use alloy_transport_http::{Client, Http};
use anyhow::anyhow;
use anyhow::Result;
use kona_client::HintType;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct BidirectionalPipe {
    pub(crate) client: Pipe,
    pub(crate) host: Pipe,
}

/// A single-direction pipe, with a read and write end.
#[derive(Debug)]
pub struct Pipe {
    pub(crate) read: PipeReader,
    pub(crate) write: PipeWriter,
}

/// Creates a [kona_host::util::BidirectionalPipe] instance.
pub fn bidirectional_pipe() -> Result<BidirectionalPipe> {
    let (ar, bw) = os_pipe::pipe().map_err(|e| anyhow!("Failed to create pipe: {e}"))?;
    let (br, aw) = os_pipe::pipe().map_err(|e| anyhow!("Failed to create pipe: {e}"))?;

    Ok(BidirectionalPipe {
        client: Pipe { read: ar, write: aw },
        host: Pipe { read: br, write: bw },
    })
}


pub(crate) fn parse_hint(s: &str) -> Result<(HintType, Bytes)> {
    let mut parts = s.split(' ').collect::<Vec<_>>();

    if parts.len() != 2 {
        anyhow::bail!("Invalid hint format: {}", s);
    }

    let hint_type = HintType::try_from(parts.remove(0))?;
    let hint_data = hex::decode(parts.remove(0)).map_err(|e| anyhow!(e))?.into();

    Ok((hint_type, hint_data))
}

pub(crate) fn http_provider(url: &str) -> ReqwestProvider {
    let url = url.parse().unwrap();
    let http = Http::<Client>::new(url);
    ReqwestProvider::new(RpcClient::new(http, true))
}

pub(crate) async fn flatten_join_result<T, E>(
    handle: JoinHandle<Result<T, E>>,
) -> Result<T, anyhow::Error>
where
    E: std::fmt::Display,
{
    match handle.await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(err)) => Err(anyhow!("{}", err)),
        Err(err) => anyhow::bail!(err),
    }
}

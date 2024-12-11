use anyhow::Result;
use tokio::sync::oneshot;

type PreimageSender = oneshot::Sender<Result<Vec<u8>>>;
type HintSender = oneshot::Sender<bool>;

pub mod client;
pub mod fetcher;
pub mod server;

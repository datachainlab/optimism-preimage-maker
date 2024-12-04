use tokio::sync::oneshot;
use anyhow::Result;

type PreimageSender = oneshot::Sender<Result<Vec<u8>>>;
type HintSender = oneshot::Sender<bool>;

pub mod client;
pub mod server;

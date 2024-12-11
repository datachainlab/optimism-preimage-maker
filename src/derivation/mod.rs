use optimism_derivation::derivation::Derivation;
use tokio::sync::oneshot;

pub mod client;
pub mod oracle;

pub type ChannelInterface = (Derivation, Option<oneshot::Sender<Vec<u8>>>);
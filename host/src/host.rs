use std::io::{stderr, stdin, stdout};
use std::os::fd::{AsFd, AsRawFd};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use anyhow::{anyhow, bail};
use futures::FutureExt;
use command_fds::{CommandFdExt, FdMapping};
use kona_common::FileDescriptor;
use kona_host::kv::KeyValueStore;
use kona_preimage::{HintReader, OracleServer, PipeHandle};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::task;
use tracing::{debug, error, info};
use crate::config::Config;
use crate::fetcher::Fetcher;
use crate::server::PreimageServer;
use crate::util;
use crate::util::Pipe;

pub async fn start_native_preimage_server<KV>(
    kv_store: Arc<RwLock<KV>>,
    fetcher: Option<Arc<RwLock<Fetcher<KV>>>>,
    hint_pipe: Pipe,
    preimage_pipe: Pipe,
) -> anyhow::Result<()>
where
    KV: KeyValueStore + Send + Sync + ?Sized + 'static,
{
    let hint_reader = HintReader::new(PipeHandle::new(
        FileDescriptor::Wildcard(hint_pipe.read.as_raw_fd() as usize),
        FileDescriptor::Wildcard(hint_pipe.write.as_raw_fd() as usize),
    ));
    let oracle_server = OracleServer::new(PipeHandle::new(
        FileDescriptor::Wildcard(preimage_pipe.read.as_raw_fd() as usize),
        FileDescriptor::Wildcard(preimage_pipe.write.as_raw_fd() as usize),
    ));

    let server = PreimageServer::new(oracle_server, hint_reader, kv_store, fetcher);
    AssertUnwindSafe(server.start())
        .catch_unwind()
        .await
        .map_err(|_| {
            error!(target: "preimage_server", "Preimage server panicked");
            anyhow!("Preimage server panicked")
        })?
        .map_err(|e| {
            error!(target: "preimage_server", "Preimage server exited with an error");
            anyhow!("Preimage server exited with an error: {:?}", e)
        })?;

    Ok(())
}



pub async fn start_server_and_native_client(cfg: Config) -> anyhow::Result<i32> {
    let hint_pipe = util::bidirectional_pipe()?;
    let preimage_pipe = util::bidirectional_pipe()?;

    let kv_store = cfg.construct_kv_store();

    let (l1_provider, blob_provider, l2_provider) = cfg.create_providers().await?;
    let fetcher = Some(Arc::new(RwLock::new(crate::fetcher::Fetcher::new(
        kv_store.clone(),
        l1_provider,
        blob_provider,
        l2_provider,
    ))));

    // Create the server and start it.
    let server_task = task::spawn(start_native_preimage_server(
        kv_store,
        fetcher,
        hint_pipe.host,
        preimage_pipe.host,
    ));

    // Start the client program in a separate child process.
    let program_task =
        task::spawn(start_native_client_program(cfg, hint_pipe.client, preimage_pipe.client));

    // Execute both tasks and wait for them to complete.
    info!("Starting preimage server and client program.");
    let exit_status;
    tokio::select!(
        r = util::flatten_join_result(server_task) => {
            r?;
            error!(target: "kona_host", "Preimage server exited before client program.");
            bail!("Host program exited before client program.");
        },
        r = util::flatten_join_result(program_task) => {
            exit_status = r?;
            debug!(target: "kona_host", "Client program has exited with status {exit_status}.");
        }
    );
    info!(target: "kona_host", "Preimage server and client program have joined.");

    Ok(exit_status)
}

pub async fn start_native_client_program(
    cfg: Config,
    hint_pipe: Pipe,
    preimage_pipe: Pipe,
) -> anyhow::Result<i32> {
    // Map the file descriptors to the standard streams and the preimage oracle and hint
    // reader's special file descriptors.
    info!("start client program {:?}", &cfg.exec);
    let mut command = Command::new(cfg.exec);
    command
        .fd_mappings(vec![
            FdMapping {
                parent_fd: stdin().as_fd().try_clone_to_owned().unwrap(),
                child_fd: FileDescriptor::StdIn.into(),
            },
            FdMapping {
                parent_fd: stdout().as_fd().try_clone_to_owned().unwrap(),
                child_fd: FileDescriptor::StdOut.into(),
            },
            FdMapping {
                parent_fd: stderr().as_fd().try_clone_to_owned().unwrap(),
                child_fd: FileDescriptor::StdErr.into(),
            },
            FdMapping {
                parent_fd: hint_pipe.read.into(),
                child_fd: FileDescriptor::HintRead.into(),
            },
            FdMapping {
                parent_fd: hint_pipe.write.into(),
                child_fd: FileDescriptor::HintWrite.into(),
            },
            FdMapping {
                parent_fd: preimage_pipe.read.into(),
                child_fd: FileDescriptor::PreimageRead.into(),
            },
            FdMapping {
                parent_fd: preimage_pipe.write.into(),
                child_fd: FileDescriptor::PreimageWrite.into(),
            },
        ])
        .expect("No errors may occur when mapping file descriptors.");

    let status = command.status().await.map_err(|e| {
        error!(target: "client_program", "Failed to execute client program: {:?}", e);
        anyhow!("Failed to execute client program: {:?}", e)
    })?;

    status.code().ok_or_else(|| anyhow!("Client program was killed by a signal."))
}

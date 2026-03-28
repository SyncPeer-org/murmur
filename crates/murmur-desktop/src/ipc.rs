//! IPC client helpers for communicating with murmurd.
//!
//! All communication uses blocking `UnixStream` wrapped in
//! `tokio::task::spawn_blocking` so iced's async runtime is never
//! blocked by socket I/O.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use iced::futures::SinkExt;
use murmur_ipc::{CliRequest, CliResponse};

/// Send a single request to murmurd and return the response.
///
/// Uses `spawn_blocking` internally so callers can `.await` from async
/// iced `Task` commands.
pub async fn send(socket_path: PathBuf, request: CliRequest) -> Result<CliResponse, String> {
    tokio::task::spawn_blocking(move || send_sync(&socket_path, &request))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

/// Synchronous send/recv on a new connection.
fn send_sync(socket_path: &Path, request: &CliRequest) -> Result<CliResponse, String> {
    let mut stream = UnixStream::connect(socket_path).map_err(|e| format!("connect: {e}"))?;
    murmur_ipc::send_message(&mut stream, request).map_err(|e| format!("send: {e}"))?;
    murmur_ipc::recv_message(&mut stream).map_err(|e| format!("recv: {e}"))
}

/// Check whether the daemon socket is connectable.
pub async fn daemon_is_running(socket_path: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || UnixStream::connect(&socket_path).is_ok())
        .await
        .unwrap_or(false)
}

/// Subscribe to the daemon event stream.
///
/// Returns an `iced::Subscription` that yields `CliResponse::Event`
/// messages as they arrive from murmurd.  The socket path is hashed
/// to give the subscription a stable identity.
pub fn event_subscription(socket_path: PathBuf) -> iced::Subscription<CliResponse> {
    iced::Subscription::run_with(socket_path, build_event_stream)
}

/// Plain `fn` pointer that creates the event stream from a socket path.
#[allow(clippy::ptr_arg)] // run_with requires fn(&D) where D=PathBuf
fn build_event_stream(
    socket_path: &PathBuf,
) -> iced::futures::stream::BoxStream<'static, CliResponse> {
    let socket_path = socket_path.clone();

    Box::pin(iced::stream::channel(64, async move |mut sender| {
        // Bridge blocking socket reads → async via tokio mpsc.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<CliResponse>(64);

        tokio::task::spawn_blocking(move || {
            let mut stream = match UnixStream::connect(&socket_path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "event stream connect failed");
                    return;
                }
            };
            if let Err(e) = murmur_ipc::send_message(&mut stream, &CliRequest::SubscribeEvents) {
                tracing::warn!(error = %e, "event stream send failed");
                return;
            }
            while let Ok(resp) = murmur_ipc::recv_message::<_, CliResponse>(&mut stream) {
                if tx.blocking_send(resp).is_err() {
                    break;
                }
            }
        });

        while let Some(resp) = rx.recv().await {
            let _ = sender.send(resp).await;
        }

        // Park forever — subscription requires a never-ending future.
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    }))
}

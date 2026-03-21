//! Gossip-based DAG sync for murmurd.
//!
//! Manages the iroh endpoint, gossip protocol subscription, and background
//! tasks for broadcasting/receiving DAG entries.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use murmur_dag::DagEntry;
use murmur_net::MurmurMessage;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Handle returned from [`start_networking`], used to broadcast entries and
/// query connected peer count.
pub struct NetworkHandle {
    /// Send serialized DAG entry bytes here to broadcast via gossip.
    pub broadcast_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Number of currently connected gossip peers.
    pub connected_peers: Arc<AtomicU64>,
    /// The iroh endpoint (kept alive for the duration of the daemon).
    _endpoint: iroh::Endpoint,
}

/// Start the networking layer: iroh endpoint, gossip subscription, and
/// background receive/broadcast tasks.
///
/// - `creator_iroh_key_bytes`: 32-byte secret key for the network creator's
///   iroh endpoint. All peers derive the same bytes from the mnemonic.
/// - `is_creator`: whether this device is the network creator.
/// - `topic`: gossip topic derived from the network ID.
pub async fn start_networking(
    engine: Arc<Mutex<murmur_engine::MurmurEngine>>,
    creator_iroh_key_bytes: [u8; 32],
    is_creator: bool,
    topic: iroh_gossip::TopicId,
) -> Result<NetworkHandle> {
    // Derive the creator's endpoint ID (all peers can compute this).
    let creator_secret = iroh::SecretKey::from_bytes(&creator_iroh_key_bytes);
    let creator_endpoint_id = creator_secret.public();

    // This device's iroh secret key: creator uses the deterministic key,
    // other devices generate a random key from 32 random bytes.
    let my_secret = if is_creator {
        creator_secret
    } else {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
        iroh::SecretKey::from_bytes(&bytes)
    };

    // Create iroh endpoint with relay enabled (for NAT traversal).
    let endpoint = iroh::Endpoint::builder()
        .secret_key(my_secret)
        .alpns(vec![iroh_gossip::ALPN.to_vec()])
        .bind()
        .await
        .context("bind iroh endpoint")?;

    info!(endpoint_id = %endpoint.id(), "iroh endpoint started");

    // Create gossip protocol.
    let gossip = iroh_gossip::Gossip::builder().spawn(endpoint.clone());

    // Accept incoming connections and route to gossip.
    let gossip_for_accept = gossip.clone();
    let ep_for_accept = endpoint.clone();
    tokio::spawn(async move {
        loop {
            let Some(incoming) = ep_for_accept.accept().await else {
                break;
            };
            let g = gossip_for_accept.clone();
            tokio::spawn(async move {
                if let Ok(connecting) = incoming.accept()
                    && let Ok(conn) = connecting.await
                {
                    let _ = g.handle_connection(conn).await;
                }
            });
        }
    });

    // Subscribe to gossip topic. Non-creator devices bootstrap with the
    // creator's endpoint ID.
    let bootstrap = if is_creator {
        vec![]
    } else {
        vec![creator_endpoint_id]
    };

    let topic_handle = gossip
        .subscribe(topic, bootstrap)
        .await
        .context("subscribe to gossip topic")?;
    let (sender, mut receiver) = topic_handle.split();

    info!(?topic, "subscribed to gossip topic");

    // Channel for outgoing entries to broadcast.
    let (broadcast_tx, mut broadcast_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // Broadcast task: reads entry bytes from channel and sends via gossip.
    let sender_for_broadcast = sender.clone();
    tokio::spawn(async move {
        while let Some(entry_bytes) = broadcast_rx.recv().await {
            let msg = MurmurMessage::DagEntryBroadcast { entry_bytes };
            let payload = msg.to_bytes();
            if let Err(e) = sender_for_broadcast
                .broadcast(bytes::Bytes::from(payload))
                .await
            {
                warn!(error = %e, "gossip broadcast failed");
            }
        }
    });

    // Connected peer tracking.
    let connected_peers = Arc::new(AtomicU64::new(0));
    let peers_for_recv = connected_peers.clone();

    // Receive task: processes incoming gossip events.
    let engine_for_recv = engine.clone();
    let sender_for_recv = sender.clone();
    tokio::spawn(async move {
        let engine = engine_for_recv;
        while let Some(event) = receiver.next().await {
            match event {
                Ok(iroh_gossip::api::Event::Received(msg)) => {
                    handle_gossip_message(&msg.content, &engine);
                }
                Ok(iroh_gossip::api::Event::NeighborUp(id)) => {
                    let count = peers_for_recv.fetch_add(1, Ordering::Relaxed) + 1;
                    info!(%id, count, "gossip peer connected");

                    // Broadcast all local entries to catch up the new peer.
                    let payloads: Vec<Vec<u8>> = {
                        let eng = engine.lock().unwrap();
                        eng.all_entries()
                            .into_iter()
                            .map(|entry| {
                                MurmurMessage::DagEntryBroadcast {
                                    entry_bytes: entry.to_bytes(),
                                }
                                .to_bytes()
                            })
                            .collect()
                    };
                    for payload in payloads {
                        if let Err(e) = sender_for_recv.broadcast(bytes::Bytes::from(payload)).await
                        {
                            warn!(error = %e, "catch-up broadcast failed");
                            break;
                        }
                    }
                    info!("broadcast existing entries to new peer");
                }
                Ok(iroh_gossip::api::Event::NeighborDown(id)) => {
                    let count = peers_for_recv.fetch_sub(1, Ordering::Relaxed) - 1;
                    info!(%id, count, "gossip peer disconnected");
                }
                Ok(_) => {}
                Err(e) => warn!(error = %e, "gossip receive error"),
            }
        }
    });

    // Collect existing DAG entries (lock scope) then broadcast.
    let existing_payloads: Vec<Vec<u8>> = {
        let eng = engine.lock().unwrap();
        eng.all_entries()
            .into_iter()
            .map(|entry| {
                let msg = MurmurMessage::DagEntryBroadcast {
                    entry_bytes: entry.to_bytes(),
                };
                msg.to_bytes()
            })
            .collect()
    };
    for payload in existing_payloads {
        if let Err(e) = sender.broadcast(bytes::Bytes::from(payload)).await {
            debug!(error = %e, "initial broadcast failed (no peers yet)");
            break;
        }
    }

    Ok(NetworkHandle {
        broadcast_tx,
        connected_peers,
        _endpoint: endpoint,
    })
}

/// Process a single gossip message.
fn handle_gossip_message(content: &[u8], engine: &Arc<Mutex<murmur_engine::MurmurEngine>>) {
    let msg = match MurmurMessage::from_bytes(content) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "failed to decode gossip message");
            return;
        }
    };

    match msg {
        MurmurMessage::DagEntryBroadcast { entry_bytes } => {
            match DagEntry::from_bytes(&entry_bytes) {
                Ok(entry) => {
                    let hash_short: String = entry
                        .hash
                        .iter()
                        .take(4)
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    let mut eng = engine.lock().unwrap();
                    match eng.receive_entry(entry) {
                        Ok(_) => info!(hash = %hash_short, "received dag entry via gossip"),
                        Err(e) => debug!(error = %e, hash = %hash_short, "gossip entry skipped"),
                    }
                }
                Err(e) => warn!(error = %e, "invalid dag entry bytes from gossip"),
            }
        }
        _ => {
            debug!("ignoring non-dag gossip message");
        }
    }
}

//! QUIC transport and gossip service for Murmur.

use bytes::Bytes;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use iroh_gossip::api::{GossipReceiver, GossipSender};
use iroh_gossip::{Gossip, TopicId};
use tracing::debug;

use crate::message::MurmurMessage;
use crate::{MAX_MESSAGE_SIZE, NetError};

// ---------------------------------------------------------------------------
// MurmurTransport
// ---------------------------------------------------------------------------

/// Wraps an iroh [`Endpoint`] for Murmur's point-to-point QUIC messaging.
///
/// Handles length-prefixed postcard messages over QUIC streams.
pub struct MurmurTransport {
    endpoint: Endpoint,
    alpn: Vec<u8>,
}

impl MurmurTransport {
    /// Create a new transport wrapping an existing endpoint.
    pub fn new(endpoint: Endpoint, alpn: Vec<u8>) -> Self {
        Self { endpoint, alpn }
    }

    /// The underlying iroh endpoint.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The ALPN protocol bytes.
    pub fn alpn(&self) -> &[u8] {
        &self.alpn
    }

    /// This endpoint's ID.
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// This endpoint's address.
    pub fn endpoint_addr(&self) -> EndpointAddr {
        self.endpoint.addr()
    }

    /// Connect to a peer and send a one-way message (uni stream).
    pub async fn send_to(
        &self,
        addr: impl Into<EndpointAddr>,
        message: &MurmurMessage,
    ) -> Result<(), NetError> {
        let conn = self
            .endpoint
            .connect(addr, &self.alpn)
            .await
            .map_err(|e| NetError::Connection(e.to_string()))?;
        send_message(&conn, message).await
    }

    /// Connect to a peer, send a request, and read a response (bi stream).
    pub async fn request_response(
        &self,
        addr: impl Into<EndpointAddr>,
        request: &MurmurMessage,
    ) -> Result<MurmurMessage, NetError> {
        let conn = self
            .endpoint
            .connect(addr, &self.alpn)
            .await
            .map_err(|e| NetError::Connection(e.to_string()))?;
        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| NetError::Connection(e.to_string()))?;
        write_message_to_send(send, request).await?;
        read_message_from_recv(recv).await
    }
}

// ---------------------------------------------------------------------------
// Length-prefixed message helpers
// ---------------------------------------------------------------------------

/// Send a length-prefixed message over a new uni stream.
pub async fn send_message(conn: &Connection, message: &MurmurMessage) -> Result<(), NetError> {
    let mut send = conn
        .open_uni()
        .await
        .map_err(|e| NetError::Connection(e.to_string()))?;
    let bytes = message.to_bytes();
    if bytes.len() > MAX_MESSAGE_SIZE {
        return Err(NetError::MessageTooLarge {
            size: bytes.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    let len = (bytes.len() as u32).to_le_bytes();
    send.write_all(&len)
        .await
        .map_err(|e| NetError::Write(e.to_string()))?;
    send.write_all(&bytes)
        .await
        .map_err(|e| NetError::Write(e.to_string()))?;
    send.finish().map_err(|e| NetError::Write(e.to_string()))?;
    Ok(())
}

/// Receive a length-prefixed message from a uni stream.
pub async fn recv_message(conn: &Connection) -> Result<MurmurMessage, NetError> {
    let mut recv = conn
        .accept_uni()
        .await
        .map_err(|e| NetError::Connection(e.to_string()))?;
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| NetError::Read(e.to_string()))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(NetError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }
    let data = recv
        .read_to_end(len)
        .await
        .map_err(|e| NetError::Read(e.to_string()))?;
    MurmurMessage::from_bytes(&data)
}

/// Write a length-prefixed message to an existing send stream.
pub async fn write_message_to_send(
    mut send: SendStream,
    message: &MurmurMessage,
) -> Result<(), NetError> {
    let bytes = message.to_bytes();
    if bytes.len() > MAX_MESSAGE_SIZE {
        return Err(NetError::MessageTooLarge {
            size: bytes.len(),
            max: MAX_MESSAGE_SIZE,
        });
    }
    let len = (bytes.len() as u32).to_le_bytes();
    send.write_all(&len)
        .await
        .map_err(|e| NetError::Write(e.to_string()))?;
    send.write_all(&bytes)
        .await
        .map_err(|e| NetError::Write(e.to_string()))?;
    send.finish().map_err(|e| NetError::Write(e.to_string()))?;
    Ok(())
}

/// Read a length-prefixed message from an existing recv stream.
pub async fn read_message_from_recv(mut recv: RecvStream) -> Result<MurmurMessage, NetError> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf)
        .await
        .map_err(|e| NetError::Read(e.to_string()))?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(NetError::MessageTooLarge {
            size: len,
            max: MAX_MESSAGE_SIZE,
        });
    }
    let data = recv
        .read_to_end(len)
        .await
        .map_err(|e| NetError::Read(e.to_string()))?;
    MurmurMessage::from_bytes(&data)
}

// ---------------------------------------------------------------------------
// GossipHandle
// ---------------------------------------------------------------------------

/// Handle to a gossip topic subscription.
///
/// Wraps the sender and receiver halves of an iroh-gossip topic.
pub struct GossipHandle {
    /// The gossip protocol instance.
    gossip: Gossip,
    /// Sender half for broadcasting.
    sender: GossipSender,
    /// Receiver half for incoming events.
    receiver: GossipReceiver,
    /// The topic ID.
    topic: TopicId,
}

impl GossipHandle {
    /// Subscribe to a gossip topic.
    ///
    /// `bootstrap` should contain the `EndpointId`s of known peers.
    pub async fn subscribe(
        gossip: Gossip,
        topic: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<Self, NetError> {
        let topic_handle = gossip
            .subscribe(topic, bootstrap)
            .await
            .map_err(|e| NetError::Gossip(e.to_string()))?;
        let (sender, receiver) = topic_handle.split();
        Ok(Self {
            gossip,
            sender,
            receiver,
            topic,
        })
    }

    /// Subscribe and wait until at least one peer is connected.
    pub async fn subscribe_and_join(
        gossip: Gossip,
        topic: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> Result<Self, NetError> {
        let topic_handle = gossip
            .subscribe_and_join(topic, bootstrap)
            .await
            .map_err(|e| NetError::Gossip(e.to_string()))?;
        let (sender, receiver) = topic_handle.split();
        Ok(Self {
            gossip,
            sender,
            receiver,
            topic,
        })
    }

    /// Broadcast a payload to all peers.
    pub async fn broadcast(&self, payload: &[u8]) -> Result<(), NetError> {
        debug!(topic = ?self.topic, len = payload.len(), "gossip: broadcasting");
        self.sender
            .broadcast(Bytes::copy_from_slice(payload))
            .await
            .map_err(|e| NetError::Gossip(e.to_string()))
    }

    /// Get a mutable reference to the receiver for polling events.
    pub fn receiver_mut(&mut self) -> &mut GossipReceiver {
        &mut self.receiver
    }

    /// The topic ID.
    pub fn topic(&self) -> TopicId {
        self.topic
    }

    /// The gossip instance.
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }
}

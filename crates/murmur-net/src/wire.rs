//! Gossip wire format utilities shared by `murmurd` and `murmur-ffi`.
//!
//! Provides compression/decompression for gossip payloads, chunked blob
//! reassembly, and associated constants.  Both the daemon and the FFI layer
//! use these functions so the wire format is always compatible.

use std::collections::HashMap;

use crate::NetError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum blob size before chunked transfer is used (4 MB).
pub const CHUNK_THRESHOLD: usize = 4 * 1024 * 1024;

/// Size of each chunk for large blob transfers (1 MB).
pub const CHUNK_SIZE: usize = 1024 * 1024;

/// Minimum payload size before compression is applied (256 bytes).
pub const COMPRESS_THRESHOLD: usize = 256;

// ---------------------------------------------------------------------------
// Compression
// ---------------------------------------------------------------------------

/// Compress a gossip wire payload if it exceeds [`COMPRESS_THRESHOLD`].
///
/// Format: `flag (1 byte) || data`.  Flag `0` = raw, flag `1` = deflate.
pub fn compress_wire(data: &[u8]) -> Vec<u8> {
    if data.len() < COMPRESS_THRESHOLD {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(0); // uncompressed
        out.extend_from_slice(data);
        return out;
    }

    use flate2::Compression;
    use flate2::write::DeflateEncoder;
    use std::io::Write;

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).expect("deflate write");
    let compressed = encoder.finish().expect("deflate finish");

    // Only use compression if it actually saves space.
    if compressed.len() < data.len() {
        let mut out = Vec::with_capacity(1 + compressed.len());
        out.push(1); // compressed
        out.extend_from_slice(&compressed);
        out
    } else {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(0);
        out.extend_from_slice(data);
        out
    }
}

/// Decompress a gossip wire payload produced by [`compress_wire`].
pub fn decompress_wire(data: &[u8]) -> Result<Vec<u8>, NetError> {
    if data.is_empty() {
        return Err(NetError::Deserialization("empty wire payload".to_string()));
    }

    match data[0] {
        0 => Ok(data[1..].to_vec()),
        1 => {
            use flate2::read::DeflateDecoder;
            use std::io::Read;

            let mut decoder = DeflateDecoder::new(&data[1..]);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| NetError::Deserialization(format!("deflate decompress: {e}")))?;
            Ok(decompressed)
        }
        flag => Err(NetError::Deserialization(format!(
            "unknown wire compression flag: {flag}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Chunk reassembly
// ---------------------------------------------------------------------------

/// In-progress chunk reassembly buffer for a single blob.
pub struct ChunkBuffer {
    total_chunks: u32,
    received: HashMap<u32, Vec<u8>>,
}

impl ChunkBuffer {
    /// Create a new buffer expecting `total_chunks` chunks.
    pub fn new(total_chunks: u32) -> Self {
        Self {
            total_chunks,
            received: HashMap::new(),
        }
    }

    /// Insert a chunk at the given index.
    pub fn insert(&mut self, index: u32, data: Vec<u8>) {
        self.received.insert(index, data);
    }

    /// Whether all expected chunks have been received.
    pub fn is_complete(&self) -> bool {
        self.received.len() == self.total_chunks as usize
    }

    /// Reassemble all chunks in order into a single byte vector.
    ///
    /// Only valid to call when [`is_complete`](Self::is_complete) returns `true`.
    pub fn reassemble(self) -> Vec<u8> {
        let mut indices: Vec<u32> = self.received.keys().copied().collect();
        indices.sort();
        let mut out = Vec::new();
        for i in indices {
            out.extend_from_slice(&self.received[&i]);
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let data = vec![42u8; 1024];
        let compressed = compress_wire(&data);
        let decompressed = decompress_wire(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_compress_small_data_not_compressed() {
        let data = vec![1u8; 100]; // < COMPRESS_THRESHOLD
        let wire = compress_wire(&data);
        assert_eq!(wire[0], 0); // flag = raw
        assert_eq!(&wire[1..], &data);
    }

    #[test]
    fn test_decompress_empty_payload_error() {
        let result = decompress_wire(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_unknown_flag_error() {
        let result = decompress_wire(&[0x42, 0x01, 0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn test_chunk_buffer_reassemble() {
        let mut buf = ChunkBuffer::new(3);
        buf.insert(0, vec![1, 2]);
        buf.insert(1, vec![3, 4]);
        assert!(!buf.is_complete());
        buf.insert(2, vec![5, 6]);
        assert!(buf.is_complete());
        assert_eq!(buf.reassemble(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_chunk_buffer_out_of_order() {
        let mut buf = ChunkBuffer::new(3);
        buf.insert(2, vec![5, 6]);
        buf.insert(0, vec![1, 2]);
        buf.insert(1, vec![3, 4]);
        assert!(buf.is_complete());
        assert_eq!(buf.reassemble(), vec![1, 2, 3, 4, 5, 6]);
    }
}

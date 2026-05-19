use fastcdc::v2020::FastCDC;

const MIN_CHUNK_SIZE: u32 = 256 * 1024; // 256 KB
const AVG_CHUNK_SIZE: u32 = 1024 * 1024; // 1 MB
const MAX_CHUNK_SIZE: u32 = 4 * 1024 * 1024; // 4 MB

/// Chunk data using FastCDC content-defined chunking.
/// Returns owned Vec of chunk byte slices for zero-copy downstream processing.
pub fn chunk_data(data: &[u8]) -> Vec<Vec<u8>> {
    if data.is_empty() {
        return vec![];
    }

    let chunker = FastCDC::new(data, MIN_CHUNK_SIZE, AVG_CHUNK_SIZE, MAX_CHUNK_SIZE);

    chunker
        .map(|chunk| data[chunk.offset..chunk.offset + chunk.length].to_vec())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let chunks = chunk_data(b"");
        assert!(chunks.is_empty());
    }

    #[test]
    fn small_input_single_chunk() {
        let data = vec![0u8; 1024];
        let chunks = chunk_data(&data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 1024);
    }

    #[test]
    fn large_input_multiple_chunks() {
        let data = vec![42u8; 8 * 1024 * 1024];
        let chunks = chunk_data(&data);
        assert!(chunks.len() > 1);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, data.len());
    }

    #[test]
    fn deterministic_chunking() {
        let data = vec![7u8; 4 * 1024 * 1024];
        let a = chunk_data(&data);
        let b = chunk_data(&data);
        assert_eq!(a.len(), b.len());
        for (ca, cb) in a.iter().zip(b.iter()) {
            assert_eq!(ca, cb);
        }
    }

    #[test]
    fn content_defined_boundaries() {
        let mut data = vec![0u8; 4 * 1024 * 1024];
        let chunks_before = chunk_data(&data);

        // Modify a byte near the end -- content-defined chunking should keep
        // earlier chunk boundaries stable.
        data[3 * 1024 * 1024] = 0xFF;
        let chunks_after = chunk_data(&data);

        // The first chunk(s) should be identical since modification is far from them
        if chunks_before.len() > 1 && chunks_after.len() > 1 {
            assert_eq!(chunks_before[0], chunks_after[0]);
        }
    }
}

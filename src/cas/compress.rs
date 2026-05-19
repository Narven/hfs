use anyhow::Result;

const COMPRESSION_LEVEL: i32 = 3;

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Ok(zstd::encode_all(data, COMPRESSION_LEVEL)?)
}

pub fn decompress(data: &[u8], original_size: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(original_size);
    let cursor = std::io::Cursor::new(data);
    let mut decoder = zstd::Decoder::new(cursor)?;
    std::io::Read::read_to_end(&mut decoder, &mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let original = b"hello world, this is a test of zstd compression in hfs";
        let compressed = compress(original).unwrap();
        let decompressed = decompress(&compressed, original.len()).unwrap();
        assert_eq!(original.as_slice(), decompressed.as_slice());
    }

    #[test]
    fn compresses_repetitive_data() {
        let original = vec![42u8; 1024 * 1024];
        let compressed = compress(&original).unwrap();
        assert!(compressed.len() < original.len() / 10);
    }

    #[test]
    fn empty_data() {
        let compressed = compress(b"").unwrap();
        let decompressed = decompress(&compressed, 0).unwrap();
        assert!(decompressed.is_empty());
    }
}

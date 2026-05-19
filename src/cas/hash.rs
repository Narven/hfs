/// BLAKE3 hashing -- SIMD-accelerated, parallelizable via tree hashing.
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

pub fn hex_to_hash(s: &str) -> anyhow::Result<[u8; 32]> {
    let bytes = hex::decode(s)?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid hash length"))?;
    Ok(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_hex() {
        let data = b"hello world";
        let hash = hash_bytes(data);
        let hex_str = hash_to_hex(&hash);
        let recovered = hex_to_hash(&hex_str).unwrap();
        assert_eq!(hash, recovered);
    }

    #[test]
    fn deterministic() {
        let a = hash_bytes(b"test data");
        let b = hash_bytes(b"test data");
        assert_eq!(a, b);
    }

    #[test]
    fn different_input_different_hash() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"world");
        assert_ne!(a, b);
    }
}

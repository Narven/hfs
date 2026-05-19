use anyhow::{Result, bail};
use std::io::{BufRead, Write};

/// Write a pkt-line text packet (adds newline).
pub fn write_text_packet(w: &mut impl Write, text: &str) -> Result<()> {
    let payload = format!("{text}\n");
    let len = payload.len() + 4; // 4 hex digits for length
    write!(w, "{len:04x}{payload}")?;
    Ok(())
}

/// Write a pkt-line data packet (raw bytes, no trailing newline).
pub fn write_data_packet(w: &mut impl Write, data: &[u8]) -> Result<()> {
    let len = data.len() + 4;
    write!(w, "{len:04x}")?;
    w.write_all(data)?;
    Ok(())
}

/// Write a flush packet (0000).
pub fn write_flush(w: &mut impl Write) -> Result<()> {
    write!(w, "0000")?;
    w.flush()?;
    Ok(())
}

/// Read a single pkt-line. Returns None on flush packet (0000).
pub fn read_packet(r: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let len_str = std::str::from_utf8(&len_buf)?;
    let len: usize = usize::from_str_radix(len_str, 16)?;

    if len == 0 {
        return Ok(None); // flush packet
    }
    if len < 4 {
        bail!("invalid pkt-line length: {len}");
    }

    let payload_len = len - 4;
    let mut payload = vec![0u8; payload_len];
    r.read_exact(&mut payload)?;

    Ok(Some(payload))
}

/// Read a text packet, stripping trailing newline. Returns None on flush.
pub fn read_text_packet(r: &mut impl BufRead) -> Result<Option<String>> {
    match read_packet(r)? {
        None => Ok(None),
        Some(data) => {
            let mut s = String::from_utf8(data)?;
            if s.ends_with('\n') {
                s.pop();
            }
            Ok(Some(s))
        }
    }
}

/// Read all packets until flush, returning them as strings.
pub fn read_until_flush(r: &mut impl BufRead) -> Result<Vec<String>> {
    let mut packets = Vec::new();
    loop {
        match read_text_packet(r)? {
            None => break,
            Some(text) => packets.push(text),
        }
    }
    Ok(packets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn write_and_read_text_packet() {
        let mut buf = Vec::new();
        write_text_packet(&mut buf, "hello").unwrap();
        write_flush(&mut buf).unwrap();

        let mut reader = std::io::BufReader::new(Cursor::new(buf));
        let text = read_text_packet(&mut reader).unwrap();
        assert_eq!(text, Some("hello".to_string()));
        let flush = read_text_packet(&mut reader).unwrap();
        assert!(flush.is_none());
    }

    #[test]
    fn read_until_flush_works() {
        let mut buf = Vec::new();
        write_text_packet(&mut buf, "line1").unwrap();
        write_text_packet(&mut buf, "line2").unwrap();
        write_flush(&mut buf).unwrap();

        let mut reader = std::io::BufReader::new(Cursor::new(buf));
        let lines = read_until_flush(&mut reader).unwrap();
        assert_eq!(lines, vec!["line1", "line2"]);
    }
}

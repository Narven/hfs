use anyhow::{Result, bail};
use std::io::{BufRead, BufReader, Write, stdin, stdout};

use crate::cas::{self, Store};
use crate::pointer::Pointer;

use super::pktline;

/// Run the long-running Git filter process protocol.
/// Called via: `git config filter.hfs.process "hfs filter-process"`
pub fn run_filter_process(store: &Store) -> Result<()> {
    let mut reader = BufReader::new(stdin().lock());
    let mut writer = stdout().lock();

    // --- Handshake: version negotiation ---
    let client_hello = pktline::read_until_flush(&mut reader)?;
    if client_hello.is_empty() || client_hello[0] != "git-filter-client" {
        bail!("expected git-filter-client, got: {client_hello:?}");
    }

    let has_version_2 = client_hello.iter().any(|l| l == "version=2");
    if !has_version_2 {
        bail!("git client does not support version 2");
    }

    pktline::write_text_packet(&mut writer, "git-filter-server")?;
    pktline::write_text_packet(&mut writer, "version=2")?;
    pktline::write_flush(&mut writer)?;

    // --- Handshake: capability negotiation ---
    let capabilities = pktline::read_until_flush(&mut reader)?;
    let has_clean = capabilities.iter().any(|c| c == "capability=clean");
    let has_smudge = capabilities.iter().any(|c| c == "capability=smudge");

    if has_clean {
        pktline::write_text_packet(&mut writer, "capability=clean")?;
    }
    if has_smudge {
        pktline::write_text_packet(&mut writer, "capability=smudge")?;
    }
    pktline::write_flush(&mut writer)?;

    // --- Command loop ---
    loop {
        let command_meta = match pktline::read_text_packet(&mut reader)? {
            Some(line) => line,
            None => break, // EOF
        };

        let command = if let Some(cmd) = command_meta.strip_prefix("command=") {
            cmd.to_string()
        } else {
            // Read remaining metadata and skip
            pktline::read_until_flush(&mut reader)?;
            read_content_packets(&mut reader)?;
            send_error_response(&mut writer, "unknown command format")?;
            continue;
        };

        // Read remaining metadata (pathname, etc.)
        let _metadata = pktline::read_until_flush(&mut reader)?;

        // Read content
        let content = read_content_packets(&mut reader)?;

        match command.as_str() {
            "clean" => {
                let result = handle_clean(store, &content)?;
                send_success_response(&mut writer, &result)?;
            }
            "smudge" => {
                let result = handle_smudge(store, &content)?;
                send_success_response(&mut writer, &result)?;
            }
            _ => {
                send_error_response(&mut writer, &format!("unsupported command: {command}"))?;
            }
        }
    }

    Ok(())
}

fn handle_clean(store: &Store, content: &[u8]) -> Result<Vec<u8>> {
    if Pointer::is_pointer(content) {
        return Ok(content.to_vec());
    }
    let (pointer, _manifest_bytes) = cas::ingest_bytes(store, content)?;
    Ok(pointer.encode().into_bytes())
}

fn handle_smudge(store: &Store, content: &[u8]) -> Result<Vec<u8>> {
    if !Pointer::is_pointer(content) {
        return Ok(content.to_vec());
    }
    let text = std::str::from_utf8(content)?;
    let pointer = Pointer::decode(text)?;
    cas::materialize(store, &pointer)
}

fn read_content_packets(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut content = Vec::new();
    loop {
        match pktline::read_packet(reader)? {
            None => break, // flush
            Some(data) => content.extend_from_slice(&data),
        }
    }
    Ok(content)
}

fn send_success_response(writer: &mut impl Write, data: &[u8]) -> Result<()> {
    pktline::write_text_packet(writer, "status=success")?;
    pktline::write_flush(writer)?;

    // Send data in chunks (max pkt-line payload is 65516 bytes)
    const MAX_PKT_DATA: usize = 65516;
    for chunk in data.chunks(MAX_PKT_DATA) {
        pktline::write_data_packet(writer, chunk)?;
    }
    pktline::write_flush(writer)?;

    // Final empty flush to signal completion
    pktline::write_flush(writer)?;
    Ok(())
}

fn send_error_response(writer: &mut impl Write, msg: &str) -> Result<()> {
    pktline::write_text_packet(writer, "status=error")?;
    pktline::write_flush(writer)?;
    tracing::error!("filter error: {msg}");
    pktline::write_flush(writer)?;
    Ok(())
}

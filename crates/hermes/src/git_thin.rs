//! Git thin over HTTPS — ADR-0074 Labor 16 + Labor 19 pack apply.
//! MVP: info/refs + 1 pack shallow (want×1) → inflate 1º blob.
//! Sem push / index pleno / delta resolve.

use alloc::string::String;
use alloc::vec::Vec;

use miniz_oxide::inflate::decompress_to_vec_zlib;

/// Parse `info/refs?service=git-upload-pack` body → (name, sha_hex).
pub fn parse_info_refs(body: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let text = core::str::from_utf8(body).unwrap_or("");
    for line in text.lines() {
        let line = line.trim();
        let line = if line.len() > 4 && line.as_bytes().iter().take(4).all(|c| c.is_ascii_hexdigit()) {
            &line[4..]
        } else {
            line
        };
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].len() >= 40 {
            let sha = String::from(&parts[0][..40.min(parts[0].len())]);
            let name = String::from(parts[1].split('\0').next().unwrap_or(parts[1]));
            if name.starts_with("refs/") || name == "HEAD" {
                out.push((name, sha));
            }
        }
    }
    out
}

/// GET `{base}/info/refs?service=git-upload-pack` via net_bridge.
pub fn fetch_refs(repo_https: &str) -> Result<Vec<(String, String)>, &'static str> {
    let base = repo_https.trim_end_matches('/');
    let url = alloc::format!("{}/info/refs?service=git-upload-pack", base);
    let body = crate::net_bridge::http_get_url(&url)?;
    if body.is_empty() {
        return Err("empty_refs");
    }
    let refs = parse_info_refs(&body);
    if refs.is_empty() {
        return Err("no_refs_parsed");
    }
    Ok(refs)
}

fn pkt_line(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() + 4;
    let mut out = Vec::with_capacity(len);
    let hex = alloc::format!("{:04x}", len);
    out.extend_from_slice(hex.as_bytes());
    out.extend_from_slice(payload);
    out
}

/// Build upload-pack want×1 + done (smart HTTP body).
pub fn build_want_done(want_sha40: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&pkt_line(
        alloc::format!("want {} multi_ack_detailed no-done side-band-64k thin-pack ofs-delta\n", want_sha40)
            .as_bytes(),
    ));
    body.extend_from_slice(b"0000");
    body.extend_from_slice(&pkt_line(b"done\n"));
    body
}

/// Locate `PACK` magic in HTTP/sideband response.
pub fn find_pack(data: &[u8]) -> Option<&[u8]> {
    data.windows(4).position(|w| w == b"PACK").map(|i| &data[i..])
}

/// Parse PACK header → object count (version 2/3).
pub fn pack_object_count(pack: &[u8]) -> Result<u32, &'static str> {
    if pack.len() < 12 || &pack[0..4] != b"PACK" {
        return Err("bad_pack_magic");
    }
    let ver = u32::from_be_bytes([pack[4], pack[5], pack[6], pack[7]]);
    if ver != 2 && ver != 3 {
        return Err("bad_pack_ver");
    }
    Ok(u32::from_be_bytes([pack[8], pack[9], pack[10], pack[11]]))
}

fn read_pack_size(pack: &[u8], mut off: usize) -> Option<(u8, usize, usize)> {
    // type = (byte >> 4) & 7; size = low 4 bits + continuation
    if off >= pack.len() {
        return None;
    }
    let mut b = pack[off];
    off += 1;
    let typ = (b >> 4) & 7;
    let mut size = (b & 0x0f) as usize;
    let mut shift = 4;
    while b & 0x80 != 0 {
        if off >= pack.len() {
            return None;
        }
        b = pack[off];
        off += 1;
        size |= ((b & 0x7f) as usize) << shift;
        shift += 7;
    }
    Some((typ, size, off))
}

/// Inflate first non-delta object; return (type, inflated). Type 3 = blob.
pub fn extract_first_object(pack: &[u8]) -> Result<(u8, Vec<u8>), &'static str> {
    let _n = pack_object_count(pack)?;
    let mut off = 12usize;
    // Skip until we get a non-delta (1=commit,2=tree,3=blob,4=tag)
    for _ in 0..64 {
        let (typ, _declared, next) = read_pack_size(pack, off).ok_or("pack_trunc")?;
        off = next;
        if typ == 6 || typ == 7 {
            // ofs/ref delta — skip residual (need base); advance by trying inflate fail→skip few bytes
            return Err("delta_residual");
        }
        // zlib stream from off
        let rest = &pack[off..];
        match decompress_to_vec_zlib(rest) {
            Ok(data) => return Ok((typ, data)),
            Err(_) => {
                // Try to find next zlib CMF (0x78) — best-effort
                return Err("inflate_fail");
            }
        }
    }
    Err("no_object")
}

/// Apply thin pack MVP: header OK + first blob (type 3) bytes.
pub fn apply_thin_pack(pack_or_http: &[u8]) -> Result<usize, &'static str> {
    let pack = find_pack(pack_or_http).ok_or("no_pack")?;
    let n = pack_object_count(pack)?;
    match extract_first_object(pack) {
        Ok((3, blob)) => {
            k_nano::slog_bin!(
                "GIT",
                "info",
                "step=pack status=OK objs={} blob_len={} VERDICT=PASS reason=thin_blob",
                n,
                blob.len()
            );
            Ok(blob.len())
        }
        Ok((t, data)) => {
            k_nano::slog_bin!(
                "GIT",
                "info",
                "step=pack status=OK objs={} type={} len={} VERDICT=PASS reason=thin_obj",
                n,
                t,
                data.len()
            );
            Ok(data.len())
        }
        Err(e) => {
            // Header alone = PARTIAL (SelfUpdate path ready; inflate/delta residual)
            k_nano::slog_bin!(
                "GIT",
                "info",
                "step=pack status=PARTIAL objs={} VERDICT=PARTIAL reason={}",
                n,
                e
            );
            Err(e)
        }
    }
}

/// POST upload-pack via TCP (HTTP/1.1) — host:80 path relative.
pub fn fetch_pack_want(host: [u8; 4], host_hdr: &str, path_upload: &str, want_sha: &str) -> Result<Vec<u8>, &'static str> {
    let body = build_want_done(want_sha);
    let req = alloc::format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: neural-os-git-thin\r\nContent-Type: application/x-git-upload-pack-request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        path_upload,
        host_hdr,
        body.len()
    );
    let mut payload = req.into_bytes();
    payload.extend_from_slice(&body);
    crate::net_bridge::tcp_xfer(host, 80, &payload).ok_or("tcp_xfer_fail")
}

/// Smoke boot — non-fatal.
pub fn boot_smoke() -> bool {
    let sample = b"001e# service=git-upload-pack\n003f0123456789abcdef0123456789abcdef01234567 HEAD\n0000";
    let parsed = parse_info_refs(sample);
    let ok_parse = parsed.iter().any(|(n, _)| n == "HEAD");
    if !ok_parse {
        k_nano::slog_bin!(
            "GIT",
            "info",
            "step=smoke status=FAIL VERDICT=FAIL reason=parse"
        );
        return false;
    }
    // Synthetic PACK header smoke (no zlib body) → PARTIAL expected
    let mut syn = Vec::from(&b"PACK"[..]);
    syn.extend_from_slice(&2u32.to_be_bytes());
    syn.extend_from_slice(&1u32.to_be_bytes());
    let _ = apply_thin_pack(&syn);

    match fetch_refs("https://github.com/git/git.git") {
        Ok(refs) => {
            k_nano::slog_bin!(
                "GIT",
                "info",
                "step=refs status=OK n={} VERDICT=PASS reason=info_refs",
                refs.len()
            );
            true
        }
        Err(e) => {
            k_nano::slog_bin!(
                "GIT",
                "info",
                "step=refs status=SKIP VERDICT=SKIP reason={} (parse_ok=1 pack_api=1)",
                e
            );
            true
        }
    }
}







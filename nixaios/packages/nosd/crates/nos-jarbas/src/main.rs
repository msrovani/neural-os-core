//! JARBAS HUD client — scaffold headless (Slint/Cage no módulo Nix).

use clap::Parser;
use nos_ipc::{Request, Response, decode_body, encode, take_frame};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "/run/nosd.sock")]
    socket: String,
    #[arg(long)]
    tcp: Option<String>,
    #[arg(long, default_value = "oi")]
    chat: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    let args = Args::parse();
    info!(target: "jarbas", "HUD scaffold connecting");
    let req = Request {
        id: 1,
        method: "chat".into(),
        caps: 1 | 16,
        cap_gen: 1,
        params: serde_json::json!({ "prompt": args.chat }),
    };
    let enc = encode(&req)?;

    let resp = if let Some(addr) = args.tcp {
        let mut stream = tokio::net::TcpStream::connect(addr).await?;
        stream.write_all(&enc).await?;
        read_resp(&mut stream).await?
    } else {
        #[cfg(unix)]
        {
            let mut stream = tokio::net::UnixStream::connect(&args.socket).await?;
            stream.write_all(&enc).await?;
            read_resp(&mut stream).await?
        }
        #[cfg(not(unix))]
        {
            let mut stream = tokio::net::TcpStream::connect("127.0.0.1:7700").await?;
            stream.write_all(&enc).await?;
            read_resp(&mut stream).await?
        }
    };

    if resp.ok {
        info!(target: "jarbas", "reply={}", resp.result);
    } else {
        warn!(target: "jarbas", "err={:?}", resp.error);
    }
    Ok(())
}

async fn read_resp<S: AsyncReadExt + Unpin>(s: &mut S) -> anyhow::Result<Response> {
    let mut acc = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = s.read(&mut buf).await?;
        if n == 0 {
            anyhow::bail!("eof");
        }
        acc.extend_from_slice(&buf[..n]);
        if let Some((body, _)) = take_frame(&acc)? {
            return Ok(decode_body(&body)?);
        }
    }
}

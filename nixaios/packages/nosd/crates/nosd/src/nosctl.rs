//! nosctl — cliente CLI do nosd.

use clap::{Parser, Subcommand};
use nos_ipc::{Request, Response, decode_body, encode, take_frame};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "/run/nosd.sock")]
    socket: String,
    #[arg(long)]
    tcp: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Health,
    Score,
    Chat { prompt: String },
    Recall { query: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let (method, params, caps) = match &args.cmd {
        Cmd::Health => ("health", serde_json::json!({}), 0u32),
        Cmd::Score => ("score", serde_json::json!({}), 0),
        Cmd::Chat { prompt } => ("chat", serde_json::json!({ "prompt": prompt }), 1 | 16), // INFER|MEMORY
        Cmd::Recall { query } => ("memory.recall", serde_json::json!({ "query": query }), 16),
    };
    let req = Request {
        id: 1,
        method: method.into(),
        caps,
        cap_gen: 1,
        params,
    };
    let enc = encode(&req)?;

    let resp = if let Some(addr) = args.tcp {
        let mut s = tokio::net::TcpStream::connect(addr).await?;
        s.write_all(&enc).await?;
        read_resp(&mut s).await?
    } else {
        #[cfg(unix)]
        {
            let mut s = tokio::net::UnixStream::connect(&args.socket).await?;
            s.write_all(&enc).await?;
            read_resp(&mut s).await?
        }
        #[cfg(not(unix))]
        {
            let mut s = tokio::net::TcpStream::connect("127.0.0.1:7700").await?;
            s.write_all(&enc).await?;
            read_resp(&mut s).await?
        }
    };
    println!("{}", serde_json::to_string_pretty(&resp)?);
    if !resp.ok {
        std::process::exit(1);
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

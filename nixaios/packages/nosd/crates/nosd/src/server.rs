use nos_core::{CapBits, CapGate, NosScore, Sev};
use nos_cortex::ModelHub;
use nos_hermes::HermesEngine;
use nos_ipc::{Request, Response, decode_body, encode, take_frame};
use nos_mesh::MeshEngine;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, warn};

pub struct AppState {
    pub hermes: HermesEngine,
    pub mesh: MeshEngine,
    pub hub: ModelHub,
    pub cap_gen: u64,
    pub shutting_down: AtomicBool,
}

pub async fn serve_tcp(addr: String, state: Arc<AppState>) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    loop {
        if state.shutting_down.load(Ordering::Relaxed) {
            break;
        }
        let (sock, _) = listener.accept().await?;
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_stream(sock, st).await {
                warn!(target: "nosd", "conn err: {e}");
            }
        });
    }
    Ok(())
}

#[cfg(unix)]
pub async fn serve_unix(path: String, state: Arc<AppState>) -> anyhow::Result<()> {
    let listener = tokio::net::UnixListener::bind(&path)?;
    loop {
        if state.shutting_down.load(Ordering::Relaxed) {
            break;
        }
        let (sock, _) = listener.accept().await?;
        let st = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_stream(sock, st).await {
                warn!(target: "nosd", "conn err: {e}");
            }
        });
    }
    Ok(())
}

async fn handle_stream<S>(mut sock: S, state: Arc<AppState>) -> anyhow::Result<()>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin,
{
    let mut acc = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = sock.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        acc.extend_from_slice(&buf[..n]);
        while let Some((body, consumed)) = take_frame(&acc)? {
            acc.drain(..consumed);
            let req: Request = decode_body(&body)?;
            let resp = dispatch(&state, req);
            sock.write_all(&encode(&resp)?).await?;
        }
    }
    Ok(())
}

fn dispatch(state: &AppState, req: Request) -> Response {
    let gate = CapGate::new(CapBits(req.caps), req.cap_gen);
    let need = req.required_cap();
    if need.0 != 0 {
        if let Err(e) = gate.allow(need, state.cap_gen) {
            return Response {
                id: req.id,
                ok: false,
                error: Some(e.into()),
                result: json!(null),
            };
        }
    }

    match req.method.as_str() {
        "health" => Response {
            id: req.id,
            ok: true,
            error: None,
            result: json!({
                "status": "ok",
                "mesh": state.mesh.health_json(),
                "hub_loaded": state.hub.loaded_count(),
            }),
        },
        "score" => {
            let s = NosScore {
                phase: "runtime".into(),
                sev: Sev::Ok,
                msg: "ok".into(),
                hub_slots: state.hub.loaded_count(),
                memory_ok: state.hermes.memory.health(),
                mesh_peers: state.mesh.peer_count(),
            };
            info!(target: "nosd", "{}", s.line());
            Response {
                id: req.id,
                ok: true,
                error: None,
                result: serde_json::to_value(&s).unwrap_or(json!(null)),
            }
        }
        "model.list" => {
            let list: Vec<_> = state
                .hub
                .list()
                .into_iter()
                .map(|(slot, st)| {
                    json!({
                        "slot": slot.name(),
                        "loaded": st.loaded,
                        "path": st.path.map(|p| p.display().to_string()),
                        "bytes": st.bytes_len,
                    })
                })
                .collect();
            Response {
                id: req.id,
                ok: true,
                error: None,
                result: json!(list),
            }
        }
        "chat" | "infer.submit" => {
            let prompt = req
                .params
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match state.hermes.chat(&gate, prompt) {
                Ok(text) => Response {
                    id: req.id,
                    ok: true,
                    error: None,
                    result: json!({ "text": text }),
                },
                Err(e) => Response {
                    id: req.id,
                    ok: false,
                    error: Some(e.to_string()),
                    result: json!(null),
                },
            }
        }
        "memory.recall" => {
            let q = req
                .params
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match state.hermes.recall_context(q) {
                Ok(texts) => Response {
                    id: req.id,
                    ok: true,
                    error: None,
                    result: json!(texts),
                },
                Err(e) => Response {
                    id: req.id,
                    ok: false,
                    error: Some(e.to_string()),
                    result: json!(null),
                },
            }
        }
        other => Response {
            id: req.id,
            ok: false,
            error: Some(format!("unknown method {other}")),
            result: json!(null),
        },
    }
}

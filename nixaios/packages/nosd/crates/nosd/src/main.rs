//! nosd — Neural-OS Daemon (NixAIOS).

mod server;

use clap::Parser;
use nos_bei::BeiRuntime;
use nos_bus::EventBus;
use nos_cortex::{CapacityProber, HotSwapController, ModelHub, scan_path};
use nos_core::{ModelSlot, NosScore, Sev};
use nos_factory::AppFactory;
use nos_hermes::HermesEngine;
use nos_memory::NosMemory;
use nos_mesh::MeshEngine;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "nosd", about = "Neural-OS Daemon (NixAIOS)")]
struct Args {
    #[arg(long, default_value = "/run/nosd.sock")]
    socket: String,
    /// TCP fallback (Windows / lab): e.g. 127.0.0.1:7700
    #[arg(long)]
    tcp: Option<String>,
    #[arg(long, default_value = "/var/lib/nosd")]
    data_dir: PathBuf,
    #[arg(long)]
    foreground: bool,
    #[arg(long, default_value_t = 15)]
    node_id: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    std::fs::create_dir_all(&args.data_dir)?;
    let mem_dir = args.data_dir.join("nsgdb");
    let models_dir = args.data_dir.join("models");
    std::fs::create_dir_all(&models_dir)?;

    let cap = CapacityProber::probe();
    let posture = nos_cortex::CapacityProber::vram_posture(&cap);
    // Sugestão honesta: só útil com llama CUDA; default nixpkgs = CPU (ngl=0).
    let suggest_ngl = nos_cortex::plan_n_gpu_layers(
        32,
        80,
        cap.vram_free_mb,
        256,
        128,
        256,
    );
    info!(
        target: "nosd",
        "capacity ram_avail_mb={} vendor={} cards={:?} render={:?} vram_free_mb={} posture={} suggest_ngl={} (CUDA llama only; NOSD_LLAMA_NGL overrides)",
        cap.ram_avail_mb,
        cap.vendor_hint,
        cap.dri_cards,
        cap.dri_render,
        cap.vram_free_mb,
        posture,
        suggest_ngl
    );

    let hub = ModelHub::new();
    for meta in scan_path(&models_dir, 64) {
        // Heurística: primeiro gguf → Active
        if meta.kind == "gguf" {
            let len = std::fs::metadata(&meta.path).map(|m| m.len()).unwrap_or(0);
            hub.register_path(ModelSlot::Active, meta.path.clone(), len);
            info!(target: "nosd", "hub register Active {}", meta.path.display());
            break;
        }
    }

    let hotswap = HotSwapController::new_cpu();
    if let Some(path) = hub.path_of(ModelSlot::Active) {
        match hotswap.try_attach_llama(path) {
            Ok(()) => info!(target: "nosd", "engine={}", hotswap.engine_name()),
            Err(e) => info!(
                target: "nosd",
                "engine=cpu-echo ({e}) — set NOSD_LLAMA_CLI + models/*.gguf for GGUF"
            ),
        }
    } else {
        info!(target: "nosd", "engine=cpu-echo (no gguf in {})", models_dir.display());
    }

    let memory = NosMemory::open(&mem_dir)?;
    let hermes = HermesEngine {
        hub: hub.clone(),
        hotswap,
        memory: memory.clone(),
        bei: BeiRuntime::new(10_000),
        bus: EventBus::new(64),
        factory: AppFactory::new(),
        cap_gen: 1,
    };
    let mesh = MeshEngine::new(args.node_id);

    let score = NosScore {
        phase: "ready".into(),
        sev: Sev::Ok,
        msg: "nosd scaffold up".into(),
        hub_slots: hub.loaded_count(),
        memory_ok: memory.health(),
        mesh_peers: mesh.peer_count(),
    };
    info!(target: "nosd", "{}", score.line());

    // sd_notify READY=1 when NOTIFY_SOCKET set (systemd)
    sd_notify_ready();

    let state = Arc::new(server::AppState {
        hermes,
        mesh,
        hub,
        data_dir: args.data_dir.clone(),
        cap_gen: 1,
        shutting_down: AtomicBool::new(false),
    });

    let bind = args.tcp.clone();
    #[cfg(unix)]
    {
        if bind.is_none() {
            let _ = std::fs::remove_file(&args.socket);
            info!(target: "nosd", "listen unix {}", args.socket);
            server::serve_unix(args.socket.clone(), state.clone()).await?;
            return Ok(());
        }
    }
    let addr = bind.unwrap_or_else(|| "127.0.0.1:7700".into());
    info!(target: "nosd", "listen tcp {}", addr);
    server::serve_tcp(addr, state).await?;
    Ok(())
}

fn sd_notify_ready() {
    let Ok(sock) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    // Minimal READY=1 without libsystemd — best-effort Unix datagram.
    #[cfg(unix)]
    {
        use std::os::unix::net::UnixDatagram;
        let path = if let Some(rest) = sock.strip_prefix('@') {
            // abstract namespace — skip in scaffold
            tracing::warn!(target: "nosd", "NOTIFY_SOCKET abstract unsupported in scaffold");
            let _ = rest;
            return;
        } else {
            sock
        };
        if let Ok(d) = UnixDatagram::unbound() {
            let _ = d.send_to(b"READY=1\n", path);
            info!(target: "nosd", "sd_notify READY=1");
        }
    }
    #[cfg(not(unix))]
    {
        let _ = sock;
    }
}

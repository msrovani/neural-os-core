//! Async I/O híbrido (ADR-0070 / Labor 11 + Labor 20).
//! Compute = ticks AgentScheduler; I/O = poll_budget cooperativo.
//! Jobs: Smoke / DelayTicks / HttpGet / TcpXfer via net_bridge.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

const MAX_JOBS: usize = 16;

/// Identificador opaco do job (0 = inválido).
pub type IoJobId = u32;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IoStatus {
    Pending,
    Ready,
    Failed,
}

enum IoKind {
    Smoke,
    DelayTicks { left: u32 },
    /// Uma tentativa por poll via net_bridge::http_get_url.
    HttpGet { url: String },
    /// DNS + TCP exchange (payload fixo).
    TcpXfer {
        host: String,
        port: u16,
        payload: Vec<u8>,
    },
    /// Labor 62: FatRead — marca Ready se FatReadable grant.
    FatRead { path: String },
}

struct Slot {
    id: IoJobId,
    kind: IoKind,
    status: IoStatus,
}

struct Runtime {
    slots: [Option<Slot>; MAX_JOBS],
    next_id: u32,
    polled: u64,
    completed: u64,
}

impl Runtime {
    const fn new() -> Self {
        Self {
            slots: [const { None }; MAX_JOBS],
            next_id: 1,
            polled: 0,
            completed: 0,
        }
    }

    fn spawn(&mut self, kind: IoKind) -> Option<IoJobId> {
        for s in self.slots.iter_mut() {
            if s.is_none() {
                let id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1).max(1);
                *s = Some(Slot {
                    id,
                    kind,
                    status: IoStatus::Pending,
                });
                return Some(id);
            }
        }
        None
    }

    fn advance_slot(slot: &mut Slot) -> bool {
        if slot.status != IoStatus::Pending {
            return false;
        }
        match &mut slot.kind {
            IoKind::Smoke => {
                slot.status = IoStatus::Ready;
                true
            }
            IoKind::DelayTicks { left } => {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    slot.status = IoStatus::Ready;
                    true
                } else {
                    false
                }
            }
            IoKind::HttpGet { url } => match crate::tls::fetch_url(url) {
                Ok(_) => {
                    slot.status = IoStatus::Ready;
                    true
                }
                Err(_) => {
                    slot.status = IoStatus::Failed;
                    true
                }
            },
            IoKind::TcpXfer {
                host,
                port,
                payload,
            } => {
                let ip = match crate::net_bridge::dns_resolve(host) {
                    Some(ip) => ip,
                    None => {
                        slot.status = IoStatus::Failed;
                        return true;
                    }
                };
                match crate::net_bridge::tcp_xfer(ip, *port, payload) {
                    Some(_) => {
                        slot.status = IoStatus::Ready;
                        true
                    }
                    None => {
                        slot.status = IoStatus::Failed;
                        true
                    }
                }
            }
            IoKind::FatRead { path } => {
                let _ = path;
                if k_hal::unlock_dag::has(k_hal::unlock_dag::CapToken::FatReadable) {
                    slot.status = IoStatus::Ready;
                } else {
                    slot.status = IoStatus::Failed;
                }
                true
            }
        }
    }

    fn poll_budget(&mut self, max: usize) -> usize {
        let mut done = 0usize;
        for s in self.slots.iter_mut() {
            if done >= max {
                break;
            }
            if let Some(slot) = s.as_mut() {
                self.polled = self.polled.wrapping_add(1);
                if Self::advance_slot(slot) {
                    self.completed = self.completed.wrapping_add(1);
                    done += 1;
                }
            }
        }
        for s in self.slots.iter_mut() {
            if let Some(slot) = s {
                if slot.status != IoStatus::Pending {
                    *s = None;
                }
            }
        }
        done
    }

    fn status_of(&self, id: IoJobId) -> Option<IoStatus> {
        for s in self.slots.iter().flatten() {
            if s.id == id {
                return Some(s.status);
            }
        }
        None
    }
}

static RT: Mutex<Runtime> = Mutex::new(Runtime::new());
static BOOT_DONE: AtomicBool = AtomicBool::new(false);
static LAST_BUDGET: AtomicU32 = AtomicU32::new(0);

pub fn spawn_smoke() -> Option<IoJobId> {
    RT.lock().spawn(IoKind::Smoke)
}

pub fn spawn_delay(ticks: u32) -> Option<IoJobId> {
    RT.lock().spawn(IoKind::DelayTicks {
        left: ticks.max(1),
    })
}

/// Enfileira HTTP GET (completa no próximo poll_budget).
pub fn spawn_http_get(url: &str) -> Option<IoJobId> {
    RT.lock().spawn(IoKind::HttpGet {
        url: String::from(url),
    })
}

/// Enfileira DNS+TCP (payload pode ser vazio).
pub fn spawn_tcp_xfer(host: &str, port: u16, payload: &[u8]) -> Option<IoJobId> {
    RT.lock().spawn(IoKind::TcpXfer {
        host: String::from(host),
        port,
        payload: payload.to_vec(),
    })
}

/// Labor 62: FatRead job.
pub fn spawn_fat_read(path: &str) -> Option<IoJobId> {
    RT.lock().spawn(IoKind::FatRead {
        path: String::from(path),
    })
}

pub fn poll_budget(max: usize) -> usize {
    let n = RT.lock().poll_budget(max);
    LAST_BUDGET.store(n as u32, Ordering::Relaxed);
    n
}

pub fn last_budget_completed() -> u32 {
    LAST_BUDGET.load(Ordering::Relaxed)
}

pub fn job_status(id: IoJobId) -> Option<IoStatus> {
    RT.lock().status_of(id)
}

pub fn boot_smoke() -> bool {
    if BOOT_DONE.swap(true, Ordering::Relaxed) {
        return true;
    }
    let Some(id) = spawn_smoke() else {
        k_nano::slog_bin!(
            "ASYNC-IO",
            "info",
            "step=smoke status=FAIL VERDICT=FAIL reason=queue_full"
        );
        return false;
    };
    let _ = spawn_delay(2);
    // SESSION_265: NÃO spawnar HttpGet/TcpXfer/FatRead no boot —
    // com net_bridge registrado isso faz DNS/TCP real e trava HW/QEMU.
    // State machine já é exercitada por Smoke + DelayTicks.
    let n = poll_budget(8);
    let _ = id;
    let ok = n >= 1;
    if ok {
        k_nano::slog_bin!(
            "ASYNC-IO",
            "info",
            "step=smoke status=OK VERDICT=PASS reason=poll_budget completed={} hybrid=ticks+io (no_live_net)",
            n
        );
    } else {
        k_nano::slog_bin!(
            "ASYNC-IO",
            "info",
            "step=smoke status=FAIL VERDICT=FAIL reason=no_progress"
        );
    }
    ok
}







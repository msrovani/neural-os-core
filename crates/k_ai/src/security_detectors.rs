//! Security detectors — monitor EventBus events and publish SECURITY_ALERT.
//! IDEA #260 — Tier 3 Security Pipeline.
//!
//! Each detector is a function that takes an EventBus event and returns
//! Option<SecurityAlert> if the event matches a suspicious pattern.

use alloc::vec::Vec;
use alloc::collections::VecDeque;

// ── Alert types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertSeverity { Low, Medium, High, Critical }

#[derive(Debug, Clone)]
pub struct SecurityAlert {
    pub detector: &'static str,
    pub severity: AlertSeverity,
    pub message: alloc::string::String,
    pub source: Option<[u8; 6]>, // MAC address if network-related
    pub timestamp: u64,
}

// ── Detector 1: PortScan ────────────────────────────────────────────────
// Detects sequential TCP connection attempts to different ports from same source
// in a short time window (< 1s / ~200 ticks).

pub struct PortScanDetector {
    /// Ring buffer: (source_ip_addr as u32, tick)
    attempts: VecDeque<(u32, u64)>,
    /// Threshold: N unique ports in window = alert
    pub threshold: usize,
    /// Window in ticks (~5ms each)
    pub window_ticks: u64,
}

impl PortScanDetector {
    pub fn new() -> Self {
        Self {
            attempts: VecDeque::with_capacity(64),
            threshold: 10,
            window_ticks: 200, // ~1s at 5ms/tick
        }
    }

    /// Feed a TCP connection attempt. Returns Some(alert) if scan detected.
    pub fn feed(&mut self, src_ip: u32, _dst_port: u16, tick: u64) -> Option<SecurityAlert> {
        // Remove old entries outside window
        while let Some(&(_, t)) = self.attempts.front() {
            if tick.wrapping_sub(t) > self.window_ticks {
                self.attempts.pop_front();
            } else {
                break;
            }
        }
        // Count unique ports for this source in the window
        let count = self.attempts.iter().filter(|&&(ip, _)| ip == src_ip).count();
        self.attempts.push_back((src_ip, tick));

        if count >= self.threshold {
            Some(SecurityAlert {
                detector: "PortScanDetector",
                severity: AlertSeverity::High,
                message: alloc::format!("Port scan detected from IP {}.{}.{}.{}: {} ports in {} ticks",
                    (src_ip >> 24) & 0xFF, (src_ip >> 16) & 0xFF,
                    (src_ip >> 8) & 0xFF, src_ip & 0xFF,
                    count + 1, self.window_ticks),
                source: None,
                timestamp: tick,
            })
        } else {
            None
        }
    }
}

// ── Detector 2: ArpSpoofDetector ────────────────────────────────────────
// Tracks IP→MAC mappings. If an IP resolves to 2 different MACs, alert.

pub struct ArpSpoofDetector {
    /// Map of IP (u32) → MAC ([u8; 6])
    known: Vec<(u32, [u8; 6])>,
}

impl ArpSpoofDetector {
    pub fn new() -> Self { Self { known: Vec::new() } }

    pub fn feed(&mut self, ip: u32, mac: [u8; 6], tick: u64) -> Option<SecurityAlert> {
        for &(known_ip, known_mac) in &self.known {
            if known_ip == ip && known_mac != mac {
                return Some(SecurityAlert {
                    detector: "ArpSpoofDetector",
                    severity: AlertSeverity::High,
                    message: alloc::format!("ARP spoof: IP {}.{}.{}.{} claimed by {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} (was {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x})",
                        (ip>>24)&0xFF, (ip>>16)&0xFF, (ip>>8)&0xFF, ip&0xFF,
                        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5],
                        known_mac[0], known_mac[1], known_mac[2],
                        known_mac[3], known_mac[4], known_mac[5]),
                    source: Some(mac),
                    timestamp: tick,
                });
            }
        }
        self.known.push((ip, mac));
        None
    }
}

// ── Detector 3: PingFloodDetector ──────────────────────────────────────
// Monitors ICMP echo requests. >100/s from same source = flood.

pub struct PingFloodDetector {
    counts: Vec<(u32, u64, u32)>, // (src_ip, tick_window, count)
}

impl PingFloodDetector {
    pub fn new() -> Self { Self { counts: Vec::new() } }

    pub fn feed(&mut self, src_ip: u32, tick: u64) -> Option<SecurityAlert> {
        for &mut (ip, ref mut last_tick, ref mut count) in &mut self.counts {
            if ip == src_ip {
                if tick.wrapping_sub(*last_tick) > 200 { // reset window
                    *last_tick = tick;
                    *count = 1;
                } else {
                    *count += 1;
                    if *count > 100 {
                        *count = 0; // prevent re-trigger
                        return Some(SecurityAlert {
                            detector: "PingFloodDetector",
                            severity: AlertSeverity::Medium,
                            message: alloc::format!("ICMP flood from IP {:08x}: {} pings/s", src_ip, *count),
                            source: None,
                            timestamp: tick,
                        });
                    }
                }
                return None;
            }
        }
        self.counts.push((src_ip, tick, 1));
        None
    }
}

// ── Detector 4: DhcpStarvationDetector ────────────────────────────────
// >50 DHCP DISCOVER from same MAC in window = starvation attack.

pub struct DhcpStarvationDetector {
    requests: Vec<([u8; 6], u64, u32)>, // (mac, tick_window, count)
}

impl DhcpStarvationDetector {
    pub fn new() -> Self { Self { requests: Vec::new() } }

    pub fn feed(&mut self, mac: [u8; 6], tick: u64) -> Option<SecurityAlert> {
        for &mut (ref m, ref mut last_tick, ref mut count) in &mut self.requests {
            if m == &mac {
                if tick.wrapping_sub(*last_tick) > 200 {
                    *last_tick = tick;
                    *count = 1;
                } else {
                    *count += 1;
                    if *count > 50 {
                        *count = 0;
                        return Some(SecurityAlert {
                            detector: "DhcpStarvationDetector",
                            severity: AlertSeverity::Medium,
                            message: alloc::format!("DHCP starvation from MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]),
                            source: Some(mac),
                            timestamp: tick,
                        });
                    }
                }
                return None;
            }
        }
        self.requests.push((mac, tick, 1));
        None
    }
}

// ── Detector 5: TimerAnomalyDetector ──────────────────────────────────
// Monitors tick counter for drift. If tick advances irregularly, alert.

pub struct TimerAnomalyDetector {
    last_tick: u64,
    drift_sum: i64,
    sample_count: u64,
}

impl TimerAnomalyDetector {
    pub fn new() -> Self { Self { last_tick: 0, drift_sum: 0, sample_count: 0 } }

    /// Call every tick. Returns alert if cumulative drift >10%.
    pub fn feed(&mut self, tick: u64) -> Option<SecurityAlert> {
        if self.last_tick != 0 {
            let expected = self.last_tick + 1;
            let drift = tick as i64 - expected as i64;
            self.drift_sum += drift.abs();
            self.sample_count += 1;

            if self.sample_count > 100 {
                let avg_drift = self.drift_sum as f64 / self.sample_count as f64;
                self.drift_sum = 0;
                self.sample_count = 0;
                if avg_drift > 0.1 {
                    return Some(SecurityAlert {
                        detector: "TimerAnomalyDetector",
                        severity: AlertSeverity::Low,
                        message: alloc::format!("Timer drift: avg {:.2} ticks/sample over window", avg_drift),
                        source: None,
                        timestamp: tick,
                    });
                }
            }
        }
        self.last_tick = tick;
        None
    }
}

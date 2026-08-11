//! SGDB Agent — bridge EventBus ↔ SGDB (ADR-0063 + versionamento de skills).
//!
//! ## Comandos (payload UTF-8, pipe-delimitado)
//! - `store_version|name|source` — versiona skill (move atual → hist, escreve novo)
//! - `rollback|name|version`    — restaura versão (ex: `v2`)
//! - `list_versions|name`       — lista versões de um skill
//! - `list_skills|`             — lista todos skills conhecidos
//! - `store_skill|name|desc`    — registra skill meta + índice
//! - `recall|query|k`           — recall semântico top-k
//!
//! ## Resposta (SGDB_RESULT)
//! - `ok|data`  — sucesso
//! - `err|msg`  — erro

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};

// ── Tópicos EventBus ──
pub const TOPIC_SGDB_CMD: &str = "SGDB_CMD";
pub const TOPIC_SGDB_RESULT: &str = "SGDB_RESULT";

const SGDB_MANIFEST: AgentManifest = AgentManifest {
    name: "sgdb_agent",
    kind: AgentKind::System,
    schedule: ScheduleKind::EventDriven,
    auto_start: true,
    persist: true,
};

pub struct SgdbAgent {
    cmd_receiver: event_bus::Receiver,
}

impl SgdbAgent {
    pub fn new() -> Self {
        SgdbAgent {
            cmd_receiver: k_nano::EVENT_BUS.subscribe(TOPIC_SGDB_CMD),
        }
    }

    /// Parses `cmd|arg1|arg2|...` → (cmd, [arg1, arg2, ...])
    fn parse_cmd(payload: &[u8]) -> (&str, Vec<&str>) {
        let text = core::str::from_utf8(payload).unwrap_or("");
        let mut parts: Vec<&str> = text.split('|').collect();
        if parts.is_empty() {
            return ("", Vec::new());
        }
        let cmd = parts.remove(0);
        (cmd, parts)
    }

    fn publish_result(&self, status: &str, data: &str) {
        let result = alloc::format!("{}|{}", status, data);
        let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
            id: 0,
            topic: String::from(TOPIC_SGDB_RESULT),
            payload: result.into_bytes(),
            token: event_bus::CapabilityToken::Legacy(1),
        });
    }

    fn handle_cmd(&mut self, payload: &[u8]) {
        let (cmd, args) = Self::parse_cmd(payload);
        k_nano::slog_hermes!("SGDB", "cmd", "cmd={} args={:?}", cmd, args);
        match cmd {
            // store_version|name|source
            "store_version" => {
                if args.len() < 2 {
                    self.publish_result("err", "store_version needs name|source");
                    return;
                }
                let name = args[0];
                let source = args[1];
                self.cmd_store_version(name, source);
            }
            // rollback|name|version
            "rollback" => {
                if args.len() < 2 {
                    self.publish_result("err", "rollback needs name|version");
                    return;
                }
                let name = args[0];
                let version = args[1];
                self.cmd_rollback(name, version);
            }
            // list_versions|name
            "list_versions" => {
                if args.is_empty() {
                    self.publish_result("err", "list_versions needs name");
                    return;
                }
                self.cmd_list_versions(args[0]);
            }
            // list_skills|
            "list_skills" => {
                self.cmd_list_skills();
            }
            // store_skill|name|desc
            "store_skill" => {
                if args.len() < 2 {
                    self.publish_result("err", "store_skill needs name|desc");
                    return;
                }
                self.cmd_store_skill(args[0], args[1]);
            }
            // recall|query|k
            "recall" => {
                if args.is_empty() {
                    self.publish_result("err", "recall needs query");
                    return;
                }
                let k = if args.len() > 1 {
                    args[1].parse::<usize>().unwrap_or(3)
                } else {
                    3
                };
                self.cmd_recall(args[0], k);
            }
            other => {
                self.publish_result("err", &alloc::format!("unknown cmd: {}", other));
            }
        }
    }

    // ── Comandos ──

    /// store_version: move curr→hist, escreve novo curr, update head
    fn cmd_store_version(&self, name: &str, source: &str) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        // Lê versão atual
        let curr_key = alloc::format!("skill/curr/{}", name);
        let old_source = k_ai::sgdb::get_kv(&curr_key).ok().flatten();

        // Determina próximo número de versão
        let head_key = alloc::format!("skill/head/{}", name);
        let next_ver = match k_ai::sgdb::get_kv(&head_key).ok().flatten() {
            Some(v) => {
                let s = core::str::from_utf8(&v).unwrap_or("v0");
                let n: u32 = s.trim_start_matches('v').parse().unwrap_or(0);
                n + 1
            }
            None => 1,
        };
        let ver_str = alloc::format!("v{}", next_ver);

        // Arquiva versão anterior no histórico
        if let Some(old) = old_source {
            let hist_key = alloc::format!("skill/hist/{}/{}", name, ver_str);
            let _ = k_ai::sgdb::put_kv(&hist_key, &old);
        }

        // Escreve novo source como curr
        let _ = k_ai::sgdb::put_kv(&curr_key, source.as_bytes());

        // Atualiza head
        let _ = k_ai::sgdb::put_kv(&head_key, ver_str.as_bytes());

        // Log
        k_nano::slog_hermes!("SGDB", "store_version", "{} → {} (hist={})", name, ver_str, next_ver);
        self.publish_result("ok", &alloc::format!("version={} old={}", ver_str, next_ver.saturating_sub(1)));
    }

    /// rollback: restaura hist/{name}/{version} para curr
    fn cmd_rollback(&self, name: &str, version: &str) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        let hist_key = alloc::format!("skill/hist/{}/{}", name, version);
        match k_ai::sgdb::get_kv(&hist_key) {
            Ok(Some(source)) => {
                let curr_key = alloc::format!("skill/curr/{}", name);
                let _ = k_ai::sgdb::put_kv(&curr_key, &source);
                let head_key = alloc::format!("skill/head/{}", name);
                let _ = k_ai::sgdb::put_kv(&head_key, version.as_bytes());
                k_nano::slog_hermes!("SGDB", "rollback", "{} → {}", name, version);
                self.publish_result("ok", version);
            }
            Ok(None) => {
                self.publish_result("err", &alloc::format!("version {} not found for {}", version, name));
            }
            Err(e) => {
                self.publish_result("err", e);
            }
        }
    }

    /// list_versions: scan ART prefix skill/hist/{name}/
    fn cmd_list_versions(&self, name: &str) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        let prefix = alloc::format!("skill/hist/{}/", name);
        let results = k_ai::sgdb::art_prefix(&prefix);
        let versions: Vec<String> = results
            .iter()
            .filter_map(|(k, _)| {
                let stripped = k.strip_prefix(&prefix)?;
                Some(stripped.to_string())
            })
            .collect();
        let joined = versions.join(",");
        k_nano::slog_hermes!("SGDB", "list_versions", "{}: {}", name, joined);
        self.publish_result("ok", &joined);
    }

    /// list_skills: lê sys/skill_index
    fn cmd_list_skills(&self) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        match k_ai::sgdb::get_kv("sys/skill_index") {
            Ok(Some(bytes)) => {
                let text = core::str::from_utf8(&bytes).unwrap_or("[]");
                self.publish_result("ok", text);
            }
            Ok(None) => {
                self.publish_result("ok", "[]");
            }
            Err(e) => {
                self.publish_result("err", e);
            }
        }
    }

    /// store_skill: put_skill_blob + append sys/skill_index
    fn cmd_store_skill(&self, name: &str, description: &str) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        // put_skill_blob
        let _ = k_ai::sgdb::put_skill_blob(name, description);

        // Atualiza índice sys/skill_index
        let mut skills: Vec<String> = match k_ai::sgdb::get_kv("sys/skill_index") {
            Ok(Some(bytes)) => {
                let text = core::str::from_utf8(&bytes).unwrap_or("[]");
                // parse simples: JSON array ["a","b"] ou CSV a,b
                if text.starts_with('[') {
                    // JSON-like — extrai strings entre aspas
                    text.split('"')
                        .enumerate()
                        .filter(|(i, _)| i % 2 == 1)
                        .map(|(_, s)| s.to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                } else {
                    text.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                }
            }
            _ => Vec::new(),
        };

        if !skills.iter().any(|s| s == name) {
            skills.push(name.to_string());
        }

        // Re-escreve índice
        let joined = skills.join(",");
        let _ = k_ai::sgdb::put_kv("sys/skill_index", joined.as_bytes());

        k_nano::slog_hermes!("SGDB", "store_skill", "{} n={}", name, skills.len());
        self.publish_result("ok", &alloc::format!("registered n={}", skills.len()));
    }

    /// recall: recall_semantic + prompt_slice
    fn cmd_recall(&self, query: &str, k: usize) {
        if !k_ai::sgdb::ready() {
            self.publish_result("err", "sgdb not ready");
            return;
        }
        // Ponytail: recall semântico precisa de embedding — hoje é stub.
        // Usa prompt_slice como fallback textual simples.
        let text = k_ai::sgdb::prompt_slice(400);
        let result = if text.is_empty() {
            String::from("(no recent context)")
        } else {
            text
        };
        k_nano::slog_hermes!("SGDB", "recall", "query={} k={} len={}", query, k, result.len());
        self.publish_result("ok", &result);
    }
}

impl Agent for SgdbAgent {
    fn manifest(&self) -> &AgentManifest {
        &SGDB_MANIFEST
    }

    fn has_pending(&self) -> bool {
        self.cmd_receiver.has_pending()
    }

    fn tick(&mut self, _tick: u64, _count: u64) -> AgentTickResult {
        while let Some(event) = self.cmd_receiver.try_receive() {
            self.handle_cmd(&event.payload);
        }
        AgentTickResult::Pending
    }
}







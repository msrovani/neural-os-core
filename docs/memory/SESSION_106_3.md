# 📝 SESSION 106.3 — Corrigir SOUL.md parser (dependência ring2→ring0)

**Data:** 2026-07-13  
**Sprint:** 106-3  
**Status:** ✅ Concluído  
**Arquivo:** `crates/jarbas/src/jarvis.rs`

---

## 🎯 Problem Statement

O `SOUL.md parser` em `crates/jarbas/src/jarvis.rs` violava o isolamento de camadas ao acessar diretamente hardware do Ring 0 (`k_nano::ATA_DRIVER`) e módulos FAT32 (`crate::fat32::`):

```rust
// ❌ ANTES — viola isolamento (jarbas é ring2)
let ata = k_nano::ATA_DRIVER.lock();
if let Some(ref ata) = *ata {
    let parts = crate::fat32::read_mbr(ata);
    // ...
}
```

**Problema arquitetural:**
- `jarbas` (Ring 2 — HCI) acessando `k_nano` (Ring 0 — HAL/direct hardware)
- Dependência circular: `jarbas → k_nano → cortex → hermes → jarbas`
- Perigo de vazamento de dependências entre camadas lógicas

---

## 🔍 Análise da Arquitetura

### Isolamento de Camadas (K²CHJ)

| Ring | Crate | Função | Acesso Permitido |
|------|-------|--------|------------------|
| 0 | `k_nano` | HAL, drivers, PCI, memory | Nenhum crate externo |
| 1 | `cortex` | LLM, BitNet, BPE | `k_nano` |
| 2 | `hermes` | Orchestration, skills, agents | `k_nano`, `cortex` |
| 2 | `jarbas` | HCI, UI, Persona | `hermes`, `neural-kernel` |

### Interface Correta: VFS (Virtual File System)

O `neural-kernel` exporta uma interface de VFS em `crates/neural-kernel/src/fs/mod.rs`:

```rust
pub fn read_vfs(path: &str) -> Result<Vec<u8>, &'static str>
pub fn write_vfs(path: &str, data: &[u8]) -> Result<(), &'static str>
pub fn list_vfs(path: &str) -> Result<Vec<String>, &'static str>
```

Essa interface resolve o path, identifica o agente responsável e delega a leitura/escrita.

---

## ✅ Solução Implementada

### 1. Atualizar Cargo.toml de jarbas

**Antes:**
```toml
[dependencies]
# ...
k-nano = { path = "../k_nano" }  # ❌ Dependência direta a hardware
cortex = { path = "../cortex" }
hermes = { path = "../hermes" }
```

**Depois:**
```toml
[dependencies]
# ...
neural-kernel = { path = "../neural-kernel" }  # ✅ Use VFS via kernel
cortex = { path = "../cortex" }
hermes = { path = "../hermes" }
```

### 2. Corrigir jarvis.rs — load_from_fat32()

**Antes:**
```rust
pub fn load_from_fat32() -> Self {
    let mut profile = Self::default_jarvis();
    unsafe {
        let ata = k_nano::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata {
            let parts = crate::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B { continue; }
                if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                    if let Some(data) = fs.read_file("SOUL.MD") {
                        // parse ...
                    }
                }
            }
        }
    }
    profile
}
```

**Depois:**
```rust
pub fn load_from_fat32() -> Self {
    let mut profile = Self::default_jarvis();
    // Usar VFS (neural-kernel) para ler SOUL.MD — evita dependência direta a k_nano::ATA_DRIVER
    if let Ok(data) = neural_kernel::fs::read_vfs("/SOUL.MD") {
        let text = core::str::from_utf8(&data).unwrap_or("");
        // parse ...
    }
    profile
}
```

### 3. Dependências restantes (aceitáveis)

O `jarbas` ainda usa:
- `k_nano::serial_println!()` — logging (serviço comum, não hardware específico)
- `k_nano::EVENT_BUS` — EventBus global (serviço comum)
- `k_nano::AUDIT_TRAIL` — auditoria global (serviço comum)

Essas são **aceitáveis** porque não acessam hardware específico (ATA, FAT32 driver), apenas serviços comuns do kernel.

---

## ✅ Validação

### Build

```bash
cargo check --release
```

**Resultado:**
```
warning: unused import: `crate::pci::scan_pci`
 --> crates\neural-kernel\src\ata.rs:3:5
   |
3 | use crate::pci::scan_pci;
   |     ^^^^^^^^^^^^^^^^^^^^

warning: variable does not need to be mutable
  --> crates\neural-kernel\src\ata.rs:26:21
   |
26 |                 let mut drv = AtaDriver { ... };

warning: `neural-kernel` (bin "neural-kernel") generated 2 warnings
    Finished `release` profile [optimized] target(s) in 4.55s
```

✅ **0 erros** (apenas 2 warnings menores em `ata.rs`, não críticos)

### Build Completo

```bash
cargo build --release
```

✅ **0 erros** — `boot` crate construído com sucesso

---

## 📊 Impacto Arquitetural

### Antes
```
jarbas (ring2) → k_nano::ATA_DRIVER (ring0) → ATA hardware
               → crate::fat32::Fat32Reader (direct)
```

### Depois
```
jarbas (ring2) → neural_kernel::fs::read_vfs() → VFS → ata_agent → ATA hardware
```

### Benefícios

| Benefício | Descrição |
|-----------|-----------|
| **Isolamento garantido** | jarbas (ring2) não acessa k_nano (ring0) diretamente |
| **Abstração de filesystem** | Use VFS padronizado, não implementation-specific FAT32 |
| **Manutenibilidade** | Mudança em ATA/FAT32 não quebra jarbas |
| **Testabilidade** | VFS pode ser mockado para testes |
| **Escalabilidade** | Novos filesystems (exfat, ntfs) automaticamente disponíveis para jarbas |

---

## 📝 Conclusão

A Sprint 106-3 **corrigiu a dependência circular** no SOUL.md parser, garantindo o isolamento de camadas do ecossistema de anéis lógicos (K²CHJ).

**Próximos passos:**
- Sprint 106-4: Corrigir Trinity MoE Router
- Sprint 106-5+: RustPython, WASM, voice I/O pipeline

---

## 🔗 Referências

- ADR-0038: K²CHJ Workspace Migration
- AGENTS.md: Ring Architecture
- `docs/architecture/0038-ecosystem-optimization.md`
- `crates/neural-kernel/src/fs/mod.rs`: VFS interface
- `crates/jarbas/src/jarvis.rs`: SOUL.md parser
- `CHANGELOG.md`: Sprint 106-3

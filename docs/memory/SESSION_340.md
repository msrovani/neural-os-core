# SESSION_340 — FAT32 root dir cache + TSC timeout + ATA BGE simplification

**Data:** 2026-09-13
**Sprint:** s340 (pós-s339)
**Status:** ✅ Implementado + testado

## Problema

Boot QEMU hangava no PHASE 5 (DriverInit) durante `lookup_file_size("BGE.BIN")`.
Causa: `lookup_file_size` caminhava a root dir cluster chain inteira via ATA PIO.
Cada cluster = 1 leitura FAT + SPC leituras de dados. Com PACK_LLM=all, 256+ clusters.
Cada leitura ATA PIO no QEMU ≈ 100-400µs → total 2-10 segundos por chamada.
Eram 11 chamadas a `lookup_file_size` no boot (BGE, Falcon3, Piper, HW Expert, RustCoder).
O guard `sz > 8MB skip ATA PIO` em main.rs:2841 **nunca era alcançado** porque o
hang acontecia *dentro* de `lookup_file_size`, antes de retornar o tamanho.

## Mudanças

### 1. TSC timeout em `lookup_file_size` + `read_file` (k_nano fat32.rs)
- Deadline de 2s via `k_nano::tsc::now_us()` no loop de caminhada da root dir.
- Retorna `None` em timeout em vez de travar para sempre.
- `read_file` também tem timeout 2s no loop de leitura dos dados do arquivo.

### 2. Cache de cluster chain da root dir (k_nano fat32.rs)
- `ROOT_DIR_CACHE`: buffer estático de 256KB (512 setores × 512B).
- `populate_root_dir_cache()`: caminha a chain UMA vez via ATA PIO, grava setores.
- `lookup_file_size`: na 1ª chamada popula cache; nas seguintes escaneia buffer.
- `find_in_root_cache()`: retorna (start_cluster, file_size) sem ler do ATA.
- `read_file`: usa cache para encontrar entrada; dados lidos normalmente (com timeout).
- Cache invalidado por `Fat32Reader::new()` (novo mount = nova chain).

### 3. Simplificação do bloco ATA BGE (main.rs)
- Bloco ATA de 40 LOC (MBR parse + Fat32Reader + lookup + read + size guard)
  substituído por 15 LOC: `read_file_from_dev(ata, "BGE.BIN")`.
- `read_file_from_dev` já faz MBR/GPT/exFAT/FAT32 parsing + usa cache + timeout.

## Impacto
- 11 chamadas `lookup_file_size` → 1 leitura ATA real + 10 cache-hit.
- Boot de ~30s → ~3s no QEMU (estimativa).
- Fallback gracioso em vez de hang.

## Validação
- `cargo check --release` — 0 erros (36 warnings pré-existentes)
- `cargo test --workspace` — hermes 188/188, k_nano OK

## Lições
- O guard `sz > 8MB skip ATA PIO` era suposto prevenir o hang mas era alcançado
  tarde demais — o hang era no FAT walk, não no read_file. Sempre instrumentar
  o ponto exato do hang antes de adicionar guards.
- Cache de metadata de filesystem é um pattern clássico: se a root dir é imutável
  durante o boot, cachear evita N caminhadas redundantes.
- `read_file_from_dev` já existia mas não era usado no path ATA — verificar se
  a abstração cobre todos os caminhos antes de re-implementar inline.

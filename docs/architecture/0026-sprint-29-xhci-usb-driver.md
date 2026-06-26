# Sprint 29 — xHCI USB Driver (Plano)

**Target:** v0.29.0

## Objetivo
Implementar driver xHCI para detectar e classificar dispositivos USB conectados,
usando o Cortex LLM para identificação automática.

## Arquitetura xHCI

```
PCI scan → class 0C03/0C0330 → BAR0 (MMIO)
  → xHCI Capability Registers (CAPLENGTH, HCSPARAMS)
  → xHCI Operational Registers (USBSTS, CRCR, DCBAAP)
  → Device Context Base Array → Slot Context → Endpoint Context
  → Event Ring → Transfer Request Blocks (TRBs)
```

## Etapas

### Etapa 1 — Detecção xHCI (~100 LOC)
- [ ] Detectar controlador xHCI no PCI scan (vendor 0x1B21, 0x8086, class 0C0330)
- [ ] Mapear MMIO via `set_page_uc()` (reutilizar de `apic.rs`)
- [ ] Ler CAPLENGTH e HCSPARAMS para descobrir número de portas/slots
- [ ] Resetar controller via USBCMD.HCRST
- [ ] Log: `[USB] xHCI detectado: 1 portas, 4 slots`

### Etapa 2 — Estruturas xHCI (~150 LOC)
- [ ] `xhci.rs` — structs para: `CapReg`, `OpReg`, `DoorbellReg`, `SlotCtx`, `EpCtx`
- [ ] `DeviceContextBaseArray` — array de ponteiros para Device Contexts
- [ ] `EventRing` — ring buffer circular de Event TRBs
- [ ] `CommandRing` — ring buffer para comandos xHCI
- [ ] Alocar e zerar Device Contexts via `BitmapFrameAllocator`

### Etapa 3 — Enumerar Portas (~100 LOC)
- [ ] Ler `PORTSC` para cada porta → detectar `CCS` (Connection Status)
- [ ] Se dispositivo conectado: ler `Port Speed` + `Port Power`
- [ ] Enviar `Address Device Command` → obter Slot ID
- [ ] Publicar `USB_DEVICE_ATTACH` no EventBus
- [ ] Log: `[USB] Dispositivo conectado: porta 1, velocidade High`

### Etapa 4 — Identificação via LLM (~50 LOC)
- [ ] Publicar `LLM_REQUEST` com vendor/device ID do USB
- [ ] Cortex LLM responde com descrição do dispositivo
- [ ] Exibir via `HERMES_RESPONSE`
- [ ] Log: `[USB] Identificado: 0781:5581 SanDisk Ultra Fit`

### Etapa 5 — Skill UsbIdentify (~30 LOC)
- [ ] Skill que lista dispositivos USB conectados
- [ ] Intent `UsbStatus` no Cortex
- [ ] `/usb` → lista dispositivos + descrição do LLM

## Estrutura do Código

```rust
// crates/neural-kernel/src/xhci.rs
pub struct XhciDriver {
    mmio_base: VirtAddr,      // BAR0 + phys_mem_offset
    cap: &'static CapReg,     // Capability Registers
    op: &'static mut OpReg,   // Operational Registers
    dcbaap: u64,             // Device Context Base Array Pointer
    slots: u8,               // Número de slots (dispositivos simultâneos)
    ports: u8,               // Número de portas USB
}

impl XhciDriver {
    pub unsafe fn new(dev: &PciDevice) -> Option<Self>;
    pub unsafe fn init(&mut self) -> bool;
    pub unsafe fn port_scan(&mut self) -> Vec<u8>; // Slot IDs com dispositivos
    pub unsafe fn get_port_speed(&self, port: u8) -> u8;
}
```

## Dependências
- `pci.rs` — já detecta class 0C03 (USB controller)
- `apic.rs` — `set_page_uc()` reutilizado para MMIO
- `memory.rs` — `PHYS_MEM_OFFSET` + `BitmapFrameAllocator`
- `cortex.rs` — LLM para identificar dispositivos

## Arquivos
| Arquivo | Ação | Linhas |
|---|---|---|
| `crates/neural-kernel/src/xhci.rs` | NOVO | ~400 |
| `crates/neural-kernel/src/main.rs` | +mod +init | +10 |
| `crates/neural-kernel/src/inventory.rs` | +USB count | +5 |
| `crates/neural-kernel/src/network_agent.rs` | +usb_event listener | +15 |

## Teste QEMU
```powershell
# USB passthrough no QEMU:
qemu-system-x86_64 -m 2G -serial stdio `
  -nic user,model=rtl8139 `
  -drive format=raw,file=bootimage-neural-kernel.bin `
  -usb -device usb-storage,drive=usb_drive `
  -drive id=usb_drive,file=fat:rw:usb_data,format=raw
```

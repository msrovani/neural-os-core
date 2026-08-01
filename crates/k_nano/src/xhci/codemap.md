# crates/k_nano/src/xhci/ — USB3 Host Controller

**Responsibility**: xHCI driver — controller init (Cap/Op/DBOFF registers), slot/port
management, hub addressing, HID boot keyboard/mouse (HID usage → PS/2 scancode), bulk
endpoints and MSC (mass storage) bring-up, plus untrusted-port disable.

**Key symbols**: `init_xhci()`, `poll_keyboard()`, `poll_mouse()`, `XHCI_STATE`,
`configure_msc_endpoints()`, `bulk_transfer()`, `disable_untrusted_ports()`;
`bringup::{bringup_boot_msc, bringup_hid_keyboard, bringup_hid_mouse}`;
`hub::{hub_ok, hub_ports, hub_address_ok, ...}`; MMIO via volatile `r32`/`w32`.

**Integration**: NIC-adjacent driver phase in bin; `globals::USB_MSC` slot (used by
`disk_agent::UsbMscCtrl` for hotplug storage) and `usb_msc::UsbMassStorage`; keyboard
feeds `scancode_to_ascii` → InputAgent path.

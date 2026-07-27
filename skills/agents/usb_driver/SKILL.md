---
name: usb_driver
division: drivers
mission: Inicializar controlador e dispositivos USB
schedule: Oneshot
native_impl: UsbDriverAgent
kind: Driver
skills: [xhci, usb]
---

# USB Driver Agent

Initializes the xHCI controller and enumerates USB devices on the bus.

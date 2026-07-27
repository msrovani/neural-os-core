---
name: platform
division: system
mission: Inicializar PCI, ACPI, APIC e SMP
schedule: Oneshot
native_impl: PlatformAgent
kind: System
skills: [pci, acpi, apic, smp]
---

# Platform Agent

Initializes the platform subsystem: PCI bus enumeration, ACPI tables, APIC and SMP coordination.

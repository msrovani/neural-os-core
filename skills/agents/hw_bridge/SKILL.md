---
name: hw_bridge
division: hardware
mission: Encaminhar eventos de hardware ao EventBus
schedule: Continuous
native_impl: HwBridgeAgent
kind: Router
skills: [irq, eventbus]
---

# HW Bridge Agent

Bridges hardware IRQ events into the EventBus for agent consumption.

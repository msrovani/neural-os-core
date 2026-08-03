# Commercial License — Neural OS Hermes

## Why Pay?

The core of Neural OS is licensed under **GNU Affero General Public License v3.0 (AGPLv3)**. This means:

| Use Case | AGPLv3 Requires |
|---|---|
| Personal / Educational / Research | ✅ Free — no restrictions |
| Open-source project or product | ✅ Free — comply with AGPLv3 (publish modifications) |
| **Proprietary product / SaaS / Embedded** | ❌ **Must purchase a Commercial License** |

A Commercial License exempts you from AGPLv3's copyleft requirements — you can integrate Neural OS into your proprietary product **without publishing your source code**.

## What You Get

| Feature | Community (AGPLv3) | Commercial |
|---|---|---|
| Full kernel source code | ✅ | ✅ |
| GPU drivers + detection | ✅ | ✅ |
| ~50 native agents | ✅ | ✅ |
| FAT32 + WASM parser | ✅ | ✅ |
| **Use in proprietary product / SaaS** | ❌ Requires AGPL compliance | ✅ **Fully exempt** |
| **Private modifications (no source release)** | ❌ | ✅ |
| **Support SLA** (response within 4h) | ❌ | ✅ |
| **Indemnification** (IP protection) | ❌ | ✅ |
| **Custom development** | ❌ | ✅ (negotiable) |
| **Hardware certification** | ❌ | ✅ (negotiable) |

## Pricing

Commercial licensing is available **upon contact**. Terms are quoted per deal based on scope of use, support requirements, and negotiation — there is no published price list.

A full acquisition / transfer of IP scenario is also possible; terms are negotiated on a case-by-case basis under NDA.

> We intentionally don't publish prices before we have paying customers anchoring value. Contact us to discuss your use case.

## Scope of the Commercial License

The commercial license covers **the Neural OS source code** (the codebase distributed under AGPLv3). It does **not** relicense third-party artifacts bundled for convenience:

- **Firmware blobs** (`firmware/`, from linux-firmware) are covered by their own vendor licenses (Realtek/AMD/Intel/Qualcomm/NVIDIA). They are redistributable in binary form under those terms, but the commercial license does not grant additional rights to them.
- **Model weights** (`models/`, e.g. Piper TTS, E5-MULTI, BGE-M3) are MIT-licensed (Piper voices carry their own dataset caveats — see ATTRIBUTIONS.md).
- **HWID datasets** derived from Windows driver packs (SDIO/WDM) have murky redistribution terms and are **not** part of what the commercial license conveys.

If you need help clearing any of these for your specific use case, we can broker the appropriate agreements.

## Warranty and Liability

**Per AGPLv3 §15 and §16, the software is provided "AS IS" without warranty of any kind.**
The Commercial License adds **no additional liability** beyond what is agreed in writing.

> *"If your company's systems crash because of Neural OS, that's your problem, not ours.
> Pay for the license, get the code. We don't guarantee your business continuity."*

## How to Purchase

Contact: [msrovani](https://github.com/msrovani) — open an issue or reach out via GitHub.

---

**Neural OS Hermes — AGPLv3 with Commercial Exception.**
*"We don't need an OS that runs AI. We need an OS that IS AI."*

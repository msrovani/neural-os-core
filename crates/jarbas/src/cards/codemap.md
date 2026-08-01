# crates/jarbas/src/cards/ — Data-only card builders (ADR-0040 #419, ADR-0079)

**Responsibility**: functions returning ready-to-render `UiDeclaration`s for
the compositor — no rendering, no direct FB access.

**Key symbols**: `storage_card::storage_card()` (disk info/usage gauge from
`k_nano::ATA_DRIVER`, id 419); `install_card::install_card()` (ADR-0079
AutoInstaller progress card).

**Integration**: consumed by the bin (e.g. SysInfoAgent) and by skills via
`desktop.spawn_card()` / `render_card()`; button actions route back through
`CARD_ACTION` (see crate map).

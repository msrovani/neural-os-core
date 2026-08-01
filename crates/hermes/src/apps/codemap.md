# crates/hermes/src/apps/

## Responsibility

Declarative app/command layer for the Hermes Chat — deliberately **no
multi-window**: apps expose commands via Hermes. Defines the `App` trait and
the global `APP_REGISTRY`.

## Key symbols

`mod.rs`: `trait App` (`name`, `description`, `on_click`, `render`,
`window_size`), `AppEntry`, `APP_REGISTRY: Mutex<BTreeMap<&'static str, AppEntry>>`,
`register_app`, `app_names`, `init_apps`. `hermes_app.rs`: `HermesApp`.
`settings_app.rs`: `SettingsApp`. `power_app.rs`: `PowerApp`.

## Integration

`init_apps()` registers the three built-ins at boot; Hermes Chat lists them
via `app_names()` and routes render/click calls through the trait. Jarbas
(display FE) can drive these apps through the registry for HMI cards.

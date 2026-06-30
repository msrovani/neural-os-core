# Sprint 63 — WWW Agents

**v0.63.0** — Internet agents built as first-class Agent/Skill entities. Each is an Agent with manifest, capabilities, and lifecycle. Browser Agent (lightweight HTTP/text), Email Agent (SMTP/POP3/IMAP), Search Agent (DuckDuckGo Lite), RSS/Feed Agent, Download Agent, WebSocket Agent.

**Base:** smoltcp TCP/IP stack (existing), DNS resolver (existing), HermesAgent shell, VFS mount (Sprint 62).

---

## Legenda
| Símbolo | Significado |
|---|---|
| 🟢 Fácil | < 100 LOC, sem dependências externas |
| 🟡 Médio | 100-300 LOC, depende de 1-2 módulos existentes |
| 🔴 Pesado | 300-700 LOC, módulo novo ou pesquisa |
| ⚫ Bloqueado | Depende de HW ou ambiente externo |

---

## Sub-Sprint 63.1 — Network Infrastructure

**Target:** v0.63.1 | **LOC:** ~400 | **Dependências:** smoltcp (existing), DNS (existing)

### Feature: Connection Pool + Async HTTP Core
- LOC: ~400
- Dependencies: smoltcp TCP sockets, DNS resolver
- Implementation:
  1. Create `net/connection_pool.rs`:
     - `struct ConnectionPool` — manages up to 16 concurrent TCP sockets
     - `open(host, port, timeout) -> ConnectionId`
     - `send(id, data) -> Result<()>` — non-blocking write
     - `recv(id, buf) -> Result<usize>` — non-blocking read, returns bytes available
     - `close(id)` — graceful shutdown (FIN)
  2. Create `net/http_client.rs`:
     - `HttpGet(url) -> Result<Response>` — parse URL, DNS resolve, TCP connect, send GET
     - `HttpPost(url, body, content_type) -> Result<Response>`
     - `Response { status: u16, headers: Vec<(String,String)>, body: Vec<u8> }`
     - Connection reuse: Keep-Alive header tracking
  3. `Url` parser: `scheme`, `host`, `port`, `path`, `query` extraction
- Files to create/modify:
  - `crates/neural-kernel/src/net/connection_pool.rs` (new)
  - `crates/neural-kernel/src/net/http_client.rs` (new)
  - `crates/neural-kernel/src/net/url.rs` (new)
  - `crates/neural-kernel/src/net/mod.rs`

---

## Sub-Sprint 63.2 — Browser Agent

**Target:** v0.63.2 | **LOC:** ~500 | **Dependências:** 63.1, Compositor (Sprint 61)

### Feature: Lightweight HTTP/Text Browser Agent
- LOC: ~500
- Dependencies: ConnectionPool, HttpClient, Compositor (for windowed output)
- Implementation:
  1. Create `agents/net/browser_agent.rs`:
     - `BrowserAgent` — implements `Agent` trait (Continuous schedule)
     - Manifest: `{ name: "BrowserAgent", kind: Network, schedule: EventDriven }`
     - Skills: `fetch_page(url) -> Markdown`, `render_page(url) -> RenderCommand[]`, `search(query) -> Results`
  2. HTML Parser (`net/html_parser.rs`):
     - Minimal SAX-style parser: tags, text, attributes
     - Extract: title, headings (`h1-h6`), paragraphs, links (`a[href]`), images (`img[alt]`), lists, tables (simplified)
     - Output: `Vec<RenderElement { kind, text, href, depth, bold, italic }>`
  3. Text Renderer:
     - Convert `RenderElement[]` to markdown-like text
     - Links numbered: `[1] https://...` at bottom
     - Images shown as `[IMG: alt text]`
     - Tables shown as aligned columns (monospace)
  4. Browser window (requires Compositor from Sprint 61):
     - URL bar at top
     - Content area: scrollable text
     - Status line: loading indicator, progress
     - Fallback: `/browse <url>` command in shell (plain text output)
  5. `/browse` command — alias for BrowserAgent.fetch_page()
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/browser_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs` (new)
  - `crates/neural-kernel/src/agents/mod.rs`
  - `crates/neural-kernel/src/net/html_parser.rs` (new)
  - `crates/neural-kernel/src/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs` (add `/browse`)

---

## Sub-Sprint 63.3 — Email Agent

**Target:** v0.63.3 | **LOC:** ~500 | **Dependências:** 63.1 (ConnectionPool + DNS)

### Feature: SMTP/POP3/IMAP Email Agent
- LOC: ~500
- Dependencies: ConnectionPool, TLS (embedded-tls or no-TLS fallback), HermesAgent notification
- Implementation:
  1. Create `agents/net/email_agent.rs`:
     - `EmailAgent` — implements Agent trait (PollEvery: 60s for IMAP/POP3)
     - Manifest: `{ name: "EmailAgent", kind: Network, schedule: PollEvery(60) }`
     - Skills: `send_email(to, subject, body)`, `read_inbox(count)`, `list_folders()`, `move_email(id, folder)`
  2. SMTP client:
     - `SmtpClient::connect(host, port)` — EHLO → AUTH LOGIN (base64) → MAIL FROM → RCPT TO → DATA → QUIT
     - Simple text transmission only (no MIME attachments initially)
  3. POP3 client:
     - `Pop3Client::connect(host, port)` — USER → PASS → LIST → RETR N → DELE N → QUIT
     - Fetch inbox listing: UIDL for unique IDs
  4. IMAP client (basic):
     - `ImapClient::connect(host, port)` — LOGIN → SELECT INBOX → FETCH 1:* (FLAGS BODY[HEADER.FIELDS (FROM SUBJECT DATE)])
     - FETCH body on demand
  5. Email storage:
     - Ring buffer: last 100 emails in memory
     - Each email: `{ id, from, to, subject, date, body, flags }`
  6. `/email` command: `email list`, `email read N`, `email send`, `email config`
  7. TLS: if `embedded-tls` crate viable, wrap connection; else fallback to cleartext with warning
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/email_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs`
  - `crates/neural-kernel/src/net/smtp.rs` (new)
  - `crates/neural-kernel/src/net/pop3.rs` (new)
  - `crates/neural-kernel/src/net/imap.rs` (new)
  - `crates/neural-kernel/src/net/email.rs` (new — Email struct)
  - `crates/neural-kernel/src/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs`

---

## Sub-Sprint 63.4 — Search Agent

**Target:** v0.63.4 | **LOC:** ~250 | **Dependências:** 63.1 (HttpClient)

### Feature: Web Search Agent
- LOC: ~250
- Dependencies: HttpClient, HtmlParser, CortexAgent (summarization)
- Implementation:
  1. Create `agents/net/search_agent.rs`:
     - `SearchAgent` — implements Agent trait (EventDriven)
     - Manifest: `{ name: "SearchAgent", kind: Network, schedule: EventDriven }`
     - Skills: `search(query) -> SearchResults`, `search_and_summarize(query) -> String`
  2. Search backends:
     - **DuckDuckGo Lite** (`lite.duckduckgo.com/lite/`): HTML page GET, parse results (no API key needed)
     - **Custom Search API** (future): JSON endpoint with API key
  3. `SearchResult { title, url, snippet }` — store in ring buffer (last 50 results)
  4. `/search <query>` command:
     - Performs search
     - Optional: feed results to CortexAgent for summarization: "Search results: ... → summarize"
  5. `/search --summary <query>` — search → fetch first N pages → LLM summarize
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/search_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs`

---

## Sub-Sprint 63.5 — RSS/Feed Agent

**Target:** v0.63.5 | **LOC:** ~350 | **Dependências:** 63.1 (HttpClient), 63.4 (SearchAgent pattern)

### Feature: RSS/Atom Feed Agent
- LOC: ~350
- Dependencies: HttpClient, CortexAgent (summarization), CronAgent (periodic poll)
- Implementation:
  1. Create `agents/net/feed_agent.rs`:
     - `FeedAgent` — implements Agent trait (PollEvery: 300s = 5 min)
     - Manifest: `{ name: "FeedAgent", kind: Network, schedule: PollEvery(300) }`
     - Skills: `subscribe(url)`, `unsubscribe(id)`, `list_feeds()`, `fetch_updates()`, `summarize(feed_id)`
  2. RSS parser (`net/rss.rs`):
     - XML minimal parser (tag-based, no full XML):
     - Parse RSS 2.0: `<channel> → <item> → <title>, <link>, <description>, <pubDate>, <guid>`
     - Parse Atom: `<feed> → <entry> → <title>, <link>, <summary>, <updated>, <id>`
     - Output: `Vec<FeedItem { feed_name, title, url, summary, date, guid, read: bool }>`
  3. Feed registry:
     - `BTreeMap<&'static str, FeedSubscription>` — URL, last_fetch, item_count
     - Persisted via BootTrustAgent config store (future: SFS)
  4. `My News Feed` ring buffer: last 100 items across all feeds
  5. `/feed` command: `feed list`, `feed add <url>`, `feed rm <id>`, `feed read`, `feed summarize`
  6. `/news` — shortcut for `feed read --unread`
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/feed_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs`
  - `crates/neural-kernel/src/net/rss.rs` (new)
  - `crates/neural-kernel/src/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs`

---

## Sub-Sprint 63.6 — Download Agent

**Target:** v0.63.6 | **LOC:** ~300 | **Dépendências:** 63.1 (HttpClient + ConnectionPool), VFS (Sprint 62)

### Feature: HTTP/FTP Download Agent
- LOC: ~300
- Dependencies: HttpClient, VFS (for storage), ConnectionPool
- Implementation:
  1. Create `agents/net/download_agent.rs`:
     - `DownloadAgent` — implements Agent trait (EventDriven)
     - Manifest: `{ name: "DownloadAgent", kind: Network, schedule: EventDriven }`
     - Skills: `download(url) -> File`, `download_to(url, path)`, `list_downloads()`, `cancel(id)`
  2. Download manager:
     - `struct Download { id, url, dest, total_bytes, received_bytes, status, callback }`
     - `DownloadQueue` — max 3 concurrent downloads, FIFO queue
     - Chunked download: `GET` with `Range:` header for resume
  3. Destination: VFS mount `/downloads/` (memory-backed ring buffer for now)
  4. Progress reporting: EventBus `DOWNLOAD_PROGRESS` events → HermesAgent → status bar
  5. `/download <url>` command
  6. `/downloads` — list active/completed downloads
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/download_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs`

---

## Sub-Sprint 63.7 — WebSocket Agent

**Target:** v0.63.7 | **LOC:** ~300 | **Dependências:** 63.1 (ConnectionPool + HttpClient)

### Feature: WebSocket Real-Time Agent
- LOC: ~300
- Dependencies: ConnectionPool, EventBus
- Implementation:
  1. Create `agents/net/websocket_agent.rs`:
     - `WebSocketAgent` — implements Agent trait (Continuous schedule)
     - Manifest: `{ name: "WebSocketAgent", kind: Network, schedule: Continuous }`
     - Skills: `connect(url)`, `send(message)`, `on_message(callback)`, `close()`, `list_connections()`
  2. WebSocket handshake:
     - HTTP Upgrade: `GET /path` → `Upgrade: websocket` → `Sec-WebSocket-Key: base64(16 bytes)` → server `Sec-WebSocket-Accept`
     - Frame parsing: opcode (text=1, binary=2, close=8, ping=9, pong=10), mask, payload length
     - Masking: client frames MUST be masked (XOR with 4-byte mask)
  3. Connection registry:
     - `WebSocketRegistry` — `BTreeMap<ConnectionId, WsConnection>`
     - Each connection: `{ id, state, rx_buffer, tx_queue }`
  4. EventBus integration:
     - `WS_MESSAGE` event published on received frames
     - HermesAgent can subscribe: `on ws message { process }`
  5. `/ws <url>` command — connect and display incoming messages
  6. `/ws send <text>` — send message on active connection
- Files to create/modify:
  - `crates/neural-kernel/src/agents/net/websocket_agent.rs` (new)
  - `crates/neural-kernel/src/agents/net/mod.rs`
  - `crates/neural-kernel/src/net/websocket.rs` (new — frame parser)
  - `crates/neural-kernel/src/net/mod.rs`
  - `crates/neural-kernel/src/hermes/commands.rs`

---

## Summary

| Sub-Sprint | Feature | LOC | Prioridade | Dependências |
|---|---|---|---|---|
| 63.1 | Network Infrastructure | ~400 | 🔴 Crítica | smoltcp, DNS |
| 63.2 | Browser Agent | ~500 | 🟡 Alta | 63.1, Compositor* |
| 63.3 | Email Agent | ~500 | 🟡 Alta | 63.1, TLS |
| 63.4 | Search Agent | ~250 | 🟢 Normal | 63.1, 63.2 (HTML) |
| 63.5 | RSS/Feed Agent | ~350 | 🟢 Normal | 63.1, 63.4 |
| 63.6 | Download Agent | ~300 | 🟢 Normal | 63.1, VFS |
| 63.7 | WebSocket Agent | ~300 | 🟢 Normal | 63.1 |
| **Total** | **7 agents** | **~2600 LOC** | | |

*Compositor optional — Browser Agent works in text mode via shell.

### Implementation Order
```
63.1 (Network Infra) ─→ 63.2 (Browser) ─→ 63.4 (Search) ─→ 63.5 (RSS)
                      ├→ 63.3 (Email)
                      ├→ 63.6 (Download)
                      └→ 63.7 (WebSocket)
```

63.1 must be first — all agents depend on `ConnectionPool` and `HttpClient`. After that, 63.2 (Browser) is the highest priority as it enables web content consumption. 63.4 (Search) depends on 63.2's `HtmlParser`. 63.3 (Email), 63.6 (Download), and 63.7 (WebSocket) are independent and can be developed in parallel after 63.1. 63.5 (RSS) depends on 63.4's search infrastructure.

### All Agents as First-Class Citizens
Every WWW entity is an Agent with:
- **Manifest**: name, kind (Network), schedule, auto_start, persist
- **Skills**: request-response capabilities registered in SkillRegistry
- **Lifecycle**: AgentScheduler manages poll intervals, watchdog, crash recovery
- **EventBus integration**: publish/receive typed events
- **Trust**: all network operations go through TrustAgent authorization

This design ensures every internet agent follows the same Agent/Skill-First architecture as kernel agents, with no special-case network daemon code.

//! Notification queue — toasts no canto superior direito.
//! ADR-0065 Fase 1.1: Urgency, source, click actions, hit-testing.
//! Gerenciado pelo DisplayAgent e renderizado no compositor.

use alloc::string::String;
use alloc::vec::Vec;
use super::compositor::draw_text;
use super::fb::DoubleBuffer;
use super::font::draw_text_bold;
use super::theme::Theme;
use super::tiling::Rect;
use k_nano::EVENT_BUS;

pub const NOTIFICATION_ACTION: &str = "NOTIFICATION_ACTION";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    /// Multiplicador de timeout baseado na urgência.
    /// Critical = 0 → nunca expira (fica até dismiss manual).
    /// Normal  = 1 → ttl_ticks padrão.
    /// Low     = 2 → dobra o tempo de exibição.
    pub fn timeout_multiplier(&self) -> u64 {
        match self {
            Urgency::Low => 2,
            Urgency::Normal => 1,
            Urgency::Critical => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u64,
    pub message: String,
    pub source: String,
    pub urgency: Urgency,
    pub expires_at: u64,
    pub dismissed: bool,
    /// Se Some, ao clicar publica `click_action` no EventBus com a mensagem como payload.
    pub click_action: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NotificationQueue {
    pub notifications: Vec<Notification>,
    pub max_visible: usize,
    pub ttl_ticks: u64,
    pub next_id: u64,
}

impl NotificationQueue {
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            max_visible: 4,
            ttl_ticks: 300, // ~3s a 100 ticks/s
            next_id: 1,
        }
    }

    /// Adiciona notificação com cálculo automático de expires_at baseado em urgência.
    pub fn push(
        &mut self,
        message: &str,
        source: &str,
        urgency: Urgency,
        click_action: Option<String>,
        now: u64,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        // ponytail: Critical usa u64::MAX (nunca expira). Adicionar hard cap se memória for preocupação.
        let expires_at = match urgency {
            Urgency::Critical => u64::MAX,
            _ => now.saturating_add(self.ttl_ticks.saturating_mul(urgency.timeout_multiplier())),
        };
        self.notifications.push(Notification {
            id,
            message: String::from(message),
            source: String::from(source),
            urgency,
            expires_at,
            dismissed: false,
            click_action,
        });
    }

    /// Remove notificações expiradas ou já dispensadas.
    /// Retorna quantidade removida.
    pub fn prune(&mut self, now: u64) -> usize {
        let before = self.notifications.len();
        self.notifications
            .retain(|n| n.expires_at > now && !n.dismissed);
        before.saturating_sub(self.notifications.len())
    }

    /// Renderiza notificações no canto superior direito.
    /// 1. Pruneia expiradas/dismissed.
    /// 2. Ordena por urgência (Critical primeiro), depois id (mais recente primeiro).
    /// 3. Desenha fundo + borda esquerda 2px colorida por urgência.
    /// 4. Linha 1: source em bold (accent) + badge de urgência.
    /// 5. Linha 2: mensagem truncada em ~36 chars.
    pub fn render(&mut self, fb: &mut DoubleBuffer, theme: &Theme, screen: Rect, now: u64) {
        self.prune(now);
        if self.notifications.is_empty() {
            return;
        }

        // Ordena por urgência (Critical first = valor enum maior primeiro)
        self.notifications.sort_by(|a, b| {
            (b.urgency as u8)
                .cmp(&(a.urgency as u8))
                .then_with(|| b.id.cmp(&a.id))
        });

        let scr_w = screen.width as usize;
        const NOTIF_W: usize = 300;
        const NOTIF_H: usize = 40;
        let x = scr_w.saturating_sub(320); // 300 + 20px padding direita
        let mut y = 4usize;

        let limit = self.max_visible.min(self.notifications.len());
        for i in 0..limit {
            let n = &self.notifications[i];

            // Background
            fb.fill_rect(
                x, y, NOTIF_W, NOTIF_H,
                theme.bg_alt.0, theme.bg_alt.1, theme.bg_alt.2,
            );

            // Borda esquerda 2px — cor conforme urgência
            let (br, bg, bb) = match n.urgency {
                Urgency::Critical => theme.error,
                Urgency::Normal => theme.accent,
                Urgency::Low => theme.fg_muted,
            };
            fb.fill_rect(x, y, 2, NOTIF_H, br, bg, bb);

            let text_x = x + 10;

            // Linha 1: source em bold (accent)
            draw_text_bold(
                fb, text_x, y + 3, &n.source, scr_w,
                theme.accent.0, theme.accent.1, theme.accent.2,
            );

            // Badge de urgência
            // ponytail: badge_x assume ~8px/char; recalcular se fonte mudar
            let badge_x = text_x + n.source.len() * 8 + 12;
            match n.urgency {
                Urgency::Critical => {
                    draw_text(
                        fb, badge_x, y + 3, "[!]", scr_w,
                        theme.error.0, theme.error.1, theme.error.2,
                    );
                }
                Urgency::Normal => {
                    fb.fill_rect(badge_x, y + 6, 6, 6, theme.accent.0, theme.accent.1, theme.accent.2);
                }
                Urgency::Low => { /* sem badge */ }
            }

            // Linha 2: mensagem truncada em ~36 chars
            // ponytail: slice assume ASCII (byte idx == char idx); usar chars() se UTF-8 multibyte aparecer
            let max_chars = 36usize;
            let display = if n.message.chars().count() > max_chars {
                let truncated: String = n.message.chars().take(max_chars).collect();
                alloc::format!("{}...", truncated)
            } else {
                n.message.clone()
            };
            draw_text(
                fb, text_x, y + 20, &display, scr_w,
                theme.fg.0, theme.fg.1, theme.fg.2,
            );

            y = y.saturating_add(NOTIF_H);
        }
    }

    /// Testa se coordenadas (cx, cy) estão sobre alguma notificação.
    /// Retorna o ID da notificação se houver acerto.
    /// Layout consistente com `render()`: x=screen_w-320, y=4, cada notif 40px.
    pub fn hit_test(&self, cx: usize, cy: usize, screen_w: usize) -> Option<u64> {
        const NOTIF_W: usize = 300;
        const NOTIF_H: usize = 40;
        let x = screen_w.saturating_sub(320);
        let y_start = 4usize;

        if cx < x || cx >= x + NOTIF_W || cy < y_start {
            return None;
        }
        let idx = (cy.saturating_sub(y_start)) / NOTIF_H;
        if idx >= self.max_visible || idx >= self.notifications.len() {
            return None;
        }
        Some(self.notifications[idx].id)
    }

    /// Marca notificação como dispensada (será removida no próximo prune/render).
    pub fn dismiss(&mut self, id: u64) {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.dismissed = true;
        }
    }

    /// Se a notificação tiver click_action, publica no EventBus.
    /// Tópico = action string, payload = mensagem da notificação em bytes.
    pub fn handle_click(&mut self, id: u64) {
        if let Some(n) = self.notifications.iter().find(|n| n.id == id) {
            if let Some(action) = &n.click_action {
                let _ = EVENT_BUS.publish(event_bus::Event {
                    id: 0,
                    topic: String::from(action),
                    payload: n.message.as_bytes().to_vec(),
                    token: event_bus::CapabilityToken::Legacy(1),
                });
            }
        }
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}

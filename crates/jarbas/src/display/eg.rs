//! ADR-0058 S1 — Adapter `embedded-graphics::DrawTarget` sobre o `DoubleBuffer`.
//!
//! Este é o **único seam** que liga o framebuffer BGRA32 (UEFI GOP) ao
//! ecossistema `embedded-graphics` (fonts, shapes, imagens, plots). O orb, o
//! dock, o HUD de relógios e o avatar continuam usando as primitivas nativas do
//! `DoubleBuffer`; o `UiRenderer` (cards) desenha via `embedded-graphics` neste
//! target. `Rgb888` → set_pixel (a ordem de canal BGRA/RGBA já é tratada por
//! `DoubleBuffer::set_pixel` via `rgb_order`).

use crate::display::fb::DoubleBuffer;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

/// Alvo de desenho embedded-graphics que escreve no back-buffer do Jarbas.
pub struct FbTarget<'a> {
    fb: &'a mut DoubleBuffer,
}

impl<'a> FbTarget<'a> {
    pub fn new(fb: &'a mut DoubleBuffer) -> Self {
        Self { fb }
    }
}

impl<'a> OriginDimensions for FbTarget<'a> {
    fn size(&self) -> Size {
        Size::new(self.fb.info.width as u32, self.fb.info.height as u32)
    }
}

impl<'a> DrawTarget for FbTarget<'a> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x >= 0 && coord.y >= 0 {
                self.fb.set_pixel(
                    coord.x as usize,
                    coord.y as usize,
                    color.r(),
                    color.g(),
                    color.b(),
                );
            }
        }
        Ok(())
    }

    /// Fast-path: preenchimento sólido delega ao `fill_rect` nativo.
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let tl = area.top_left;
        if tl.x >= 0 && tl.y >= 0 {
            self.fb.fill_rect(
                tl.x as usize,
                tl.y as usize,
                area.size.width as usize,
                area.size.height as usize,
                color.r(),
                color.g(),
                color.b(),
            );
        }
        Ok(())
    }
}

/// Self-test de boot (sem modelo): desenha um retângulo + texto via
/// embedded-graphics num canto do FB e confirma que o adapter funciona.
pub fn self_test(fb: &mut DoubleBuffer) -> bool {
    use embedded_graphics::{
        mono_font::{ascii::FONT_6X10, MonoTextStyle},
        primitives::{PrimitiveStyle, Rectangle as Rect},
        text::Text,
    };
    let mut t = FbTarget::new(fb);
    let ok_rect = Rect::new(Point::new(2, 2), Size::new(120, 16))
        .into_styled(PrimitiveStyle::with_fill(Rgb888::new(0, 40, 90)))
        .draw(&mut t)
        .is_ok();
    let style = MonoTextStyle::new(&FONT_6X10, Rgb888::new(220, 245, 255));
    let ok_text = Text::new("embedded-graphics OK", Point::new(4, 14), style)
        .draw(&mut t)
        .is_ok();
    let ok = ok_rect && ok_text;
    if ok {
        k_nano::slog_jarbas!("UI", "info", "embedded-graphics DrawTarget self-test PASS (ADR-0058 S1)");
    } else {
        k_nano::slog_jarbas!("UI", "warn", "embedded-graphics self-test FAIL");
    }
    ok
}

//! Botão clicável — reconstruído para o novo visual (`PIIramid Layout.dc.html`):
//! retângulo com borda grossa e sombra deslocada, sem depender de textura.
//! Cada variante espelha um dos estilos do "Guia de Estilo" (Primária,
//! Secundária, Perigo, Fantasma).

use macroquad::prelude::*;

use crate::ui::theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    /// fundo claro, borda dourada — ação principal (EXECUTAR)
    Primary,
    /// fundo escuro, borda de areia — ação secundária (FUGIR, MENU)
    Secondary,
    /// fundo vermelho escuro, borda de sangue — ação destrutiva/corrigir erro
    Danger,
    /// sem preenchimento, borda tracejada-like fina — ação terciária
    Ghost,
}

pub struct Button {
    pub label: String,
    pub position: Vec2,
    pub size: Vec2,
    pub style: ButtonStyle,
    pub font_size: u16,
    pub hovered: bool,
    /// RFC-009 regra 2: botão desabilitado não responde a clique e é
    /// desenhado dessaturado — sem isso o botão Executar parecia clicável
    /// durante `Phase::Executing` mesmo estando fora da lógica de clique.
    pub disabled: bool,
}

impl Button {
    pub fn new(label: &str, position: Vec2, size: Vec2, style: ButtonStyle, font_size: u16) -> Self {
        Button { label: label.to_string(), position, size, style, font_size, hovered: false, disabled: false }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, self.size.x, self.size.y)
    }

    pub fn update_hover(&mut self, mouse: Vec2) {
        self.hovered = self.rect().contains(mouse);
    }

    pub fn clicked(&self, mouse: Vec2) -> bool {
        !self.disabled && self.hovered && is_mouse_button_pressed(MouseButton::Left) && self.rect().contains(mouse)
    }

    fn colors(&self) -> (Color, Color, Color) {
        // (fundo, borda, texto)
        match self.style {
            ButtonStyle::Primary => {
                if self.hovered {
                    (Color::new(0.97, 0.87, 0.6, 1.0), theme::OURO, theme::PEDRA)
                } else {
                    (theme::PAPIRO, theme::OURO, theme::PEDRA)
                }
            }
            ButtonStyle::Secondary => {
                if self.hovered {
                    (theme::TIJOLO, theme::POEIRA, theme::PAPIRO)
                } else {
                    (theme::PEDRA, theme::AREIA_ESCURA, theme::POEIRA)
                }
            }
            ButtonStyle::Danger => {
                if self.hovered {
                    (Color::new(0.29, 0.12, 0.1, 1.0), theme::SANGUE, WHITE)
                } else {
                    (theme::DANGER_BG, theme::SANGUE, Color::new(0.94, 0.66, 0.6, 1.0))
                }
            }
            ButtonStyle::Ghost => {
                if self.hovered {
                    (Color::new(0.0, 0.0, 0.0, 0.0), theme::PAPIRO, theme::PAPIRO)
                } else {
                    (Color::new(0.0, 0.0, 0.0, 0.0), theme::POEIRA, theme::POEIRA)
                }
            }
        }
    }

    pub fn draw(&self, font: &Font) {
        let (bg, border, text_color) = self.colors();
        // RFC-009 regra 2: dessatura em vez de inventar uma 5ª variante —
        // mesma cor de fundo/borda/texto da variante original, alfa a ~60%.
        let (bg, border, text_color) = if self.disabled {
            (fade(bg), fade(border), fade(text_color))
        } else {
            (bg, border, text_color)
        };
        let border_w = if self.style == ButtonStyle::Ghost { 2.0 } else { 4.0 };

        if self.style != ButtonStyle::Ghost {
            let shadow_off = if self.hovered { 3.0 } else { 6.0 };
            draw_rectangle(
                self.position.x + shadow_off,
                self.position.y + shadow_off,
                self.size.x,
                self.size.y,
                Color::new(0.0, 0.0, 0.0, 0.5),
            );
        }
        if bg.a > 0.0 {
            draw_rectangle(self.position.x, self.position.y, self.size.x, self.size.y, bg);
        }
        draw_rectangle_lines(self.position.x, self.position.y, self.size.x, self.size.y, border_w, border);

        let dims = measure_text(&self.label, Some(font), self.font_size, 1.0);
        let x = self.position.x + (self.size.x - dims.width) / 2.0;
        let y = self.position.y + (self.size.y + dims.height) / 2.0;
        draw_text_ex(&self.label, x, y, TextParams { font: Some(font), font_size: self.font_size, color: text_color, ..Default::default() });
    }
}

/// reduz o alfa em ~40% mantendo o RGB — é o "filtro" de estado
/// desabilitado citado na RFC-009, aplicado igual às 4 variantes.
fn fade(c: Color) -> Color {
    Color::new(c.r, c.g, c.b, c.a * 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_button_never_reports_clicked() {
        let mut b = Button::new("X", vec2(0.0, 0.0), vec2(10.0, 10.0), ButtonStyle::Primary, 10);
        b.disabled = true;
        b.hovered = true;
        assert!(!b.clicked(vec2(5.0, 5.0)));
    }
}

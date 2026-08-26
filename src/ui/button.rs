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
}

impl Button {
    pub fn new(label: &str, position: Vec2, size: Vec2, style: ButtonStyle, font_size: u16) -> Self {
        Button { label: label.to_string(), position, size, style, font_size, hovered: false }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, self.size.x, self.size.y)
    }

    pub fn update_hover(&mut self, mouse: Vec2) {
        self.hovered = self.rect().contains(mouse);
    }

    pub fn clicked(&self, mouse: Vec2) -> bool {
        self.hovered && is_mouse_button_pressed(MouseButton::Left) && self.rect().contains(mouse)
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

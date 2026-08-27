//! Guia de estilo — porta ~1:1 da tela "Guia de Estilo" de
//! `PIIramid Layout.dc.html`: referência visual navegável (paleta,
//! tipografia, molduras, botões, barras), não depende de estado de jogo.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::WIDTH;
use crate::scenes::Transition;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

pub struct StyleGuideScene {
    btn_back: Button,
}

impl StyleGuideScene {
    pub fn new() -> Self {
        StyleGuideScene { btn_back: Button::new("VOLTAR", vec2(WIDTH - 160.0, 14.0), vec2(140.0, 40.0), ButtonStyle::Ghost, 13) }
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = mouse_position().into();
        self.btn_back.update_hover(mouse);
        if self.btn_back.clicked(mouse) || is_key_pressed(KeyCode::Escape) {
            return Some(Transition::GoToMenu { last_drop: None });
        }
        None
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);

        draw_text_ex("GUIA DE ESTILO", 40.0, 46.0, TextParams { font: Some(&assets.font_title), font_size: 26, color: theme::OURO, ..Default::default() });
        draw_text_ex(
            "PIIRAMID - UI KIT",
            40.0,
            68.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::POEIRA, ..Default::default() },
        );
        self.btn_back.draw(&assets.font_body);

        self.draw_palette(assets, 40.0, 100.0);
        self.draw_typography(assets, 40.0, 280.0);
        self.draw_frames(assets, 660.0, 280.0);
        self.draw_buttons(assets, 40.0, 490.0);
        self.draw_bars(assets, 660.0, 490.0);
    }

    fn draw_palette(&self, assets: &Assets, x: f32, y: f32) {
        draw_text_ex("PALETA", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let cols = 12;
        let cell_w = (WIDTH - 80.0) / cols as f32;
        for (i, (name, hex, color)) in theme::PALETTE.iter().enumerate() {
            let cx = x + i as f32 * cell_w;
            draw_rectangle(cx, y + 20.0, cell_w - 8.0, 60.0, *color);
            draw_rectangle_lines(cx, y + 20.0, cell_w - 8.0, 60.0, 2.0, theme::TIJOLO);
            draw_text_ex(name, cx, y + 96.0, TextParams { font: Some(&assets.font_body), font_size: 11, color: theme::PAPIRO, ..Default::default() });
            draw_text_ex(hex, cx, y + 110.0, TextParams { font: Some(&assets.font_body), font_size: 10, color: theme::POEIRA, ..Default::default() });
        }
    }

    fn draw_typography(&self, assets: &Assets, x: f32, y: f32) {
        draw_text_ex("TIPOGRAFIA", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let w = 580.0;
        draw_rectangle(x, y + 16.0, w, 180.0, theme::PEDRA);
        draw_rectangle_lines(x, y + 16.0, w, 180.0, 2.0, theme::TIJOLO);

        draw_text_ex(
            "PRESS START 2P",
            x + 16.0,
            y + 40.0,
            TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() },
        );
        draw_text_ex("MUMIA", x + 16.0, y + 76.0, TextParams { font: Some(&assets.font_title), font_size: 32, color: theme::PAPIRO, ..Default::default() });
        draw_text_ex("EXECUTAR", x + 16.0, y + 100.0, TextParams { font: Some(&assets.font_title), font_size: 14, color: theme::OURO, ..Default::default() });

        draw_text_ex(
            "SILKSCREEN",
            x + 16.0,
            y + 130.0,
            TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() },
        );
        draw_text_ex(
            "POSTURA: GUARDA",
            x + 16.0,
            y + 152.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::PAPIRO, ..Default::default() },
        );
        draw_text_ex(
            "Registro do turno",
            x + 16.0,
            y + 174.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );
    }

    fn draw_frames(&self, assets: &Assets, x: f32, y: f32) {
        draw_text_ex("MOLDURAS", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let frames: [(&str, Color, Color); 4] =
            [("PRIMARIA", theme::PEDRA, theme::OURO), ("SECUNDARIA", theme::PEDRA, theme::TIJOLO), ("PERIGO", theme::DANGER_BG, theme::SANGUE), ("SCRIPT", theme::OK_BG, theme::ESCARAVELHO)];
        let w = 280.0;
        let h = 60.0;
        for (i, (label, bg, border)) in frames.iter().enumerate() {
            let cx = x + (i % 2) as f32 * (w + 16.0);
            let cy = y + 16.0 + (i / 2) as f32 * (h + 12.0);
            draw_rectangle(cx, cy, w, h, *bg);
            draw_rectangle_lines(cx, cy, w, h, 3.0, *border);
            let dims = measure_text(label, Some(&assets.font_body), 14, 1.0);
            draw_text_ex(label, cx + (w - dims.width) / 2.0, cy + h / 2.0 + 5.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: *border, ..Default::default() });
        }
    }

    fn draw_buttons(&self, assets: &Assets, x: f32, y: f32) {
        draw_text_ex("BOTOES", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let sample = |label: &str, style: ButtonStyle, cx: f32| {
            let b = Button::new(label, vec2(cx, y + 16.0), vec2(180.0, 56.0), style, 15);
            b.draw(&assets.font_title);
        };
        sample("EXECUTAR", ButtonStyle::Primary, x);
        sample("FUGIR", ButtonStyle::Secondary, x + 200.0);
        sample("CORRIGIR", ButtonStyle::Danger, x + 400.0);
    }

    fn draw_bars(&self, assets: &Assets, x: f32, y: f32) {
        draw_text_ex("BARRAS E REALCE", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let w = 560.0;
        draw_rectangle(x, y + 16.0, w, 96.0, theme::PEDRA);
        draw_rectangle_lines(x, y + 16.0, w, 96.0, 2.0, theme::TIJOLO);

        draw_rectangle(x + 16.0, y + 32.0, w - 32.0, 18.0, theme::TUMBA);
        draw_rectangle(x + 16.0, y + 32.0, (w - 32.0) * 0.72, 18.0, theme::VIDA);
        draw_rectangle(x + 16.0, y + 56.0, w - 32.0, 18.0, theme::TUMBA);
        draw_rectangle(x + 16.0, y + 56.0, (w - 32.0) * 0.44, 18.0, theme::SANGUE);
        draw_rectangle(x + 16.0, y + 80.0, w - 32.0, 14.0, theme::TUMBA);
        draw_rectangle(x + 16.0, y + 80.0, (w - 32.0) * 0.6, 14.0, theme::ESCARAVELHO);

        let words: [(&str, Color); 6] =
            [("SE / SENAO / REPETIR", theme::ESCARAVELHO), ("ESPADA MAGIA", theme::ESCARAVELHO), ("40", theme::MUSGO), ("Fogo", theme::MUSGO), ("( ) { }", theme::POEIRA), ("ERRO", theme::SANGUE)];
        let mut wx = x;
        for (word, color) in words {
            draw_text_ex(word, wx, y + 128.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color, ..Default::default() });
            let dims = measure_text(word, Some(&assets.font_body), theme::BODY_LG, 1.0);
            wx += dims.width + 20.0;
        }
    }
}

//! Tela de menu — porta de `PIIramid Layout.dc.html` (tela "Menu"): duas
//! colunas, título+descrição+lista de navegação à esquerda, key art com
//! gradiente à direita. Substitui o menu centrado anterior.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::inventory::SaveData;
use crate::scenes::Transition;
use crate::ui::theme;

const LEFT_W: f32 = 620.0;

/// Ação de cada item do menu, mais leve que `Transition` (que agora carrega
/// `SaveData` — RFC-002). `items()` roda todo frame só pra detectar hover;
/// construir `Transition::GoToOverworld` ali significaria ler/zerar o save
/// a cada frame com o menu aberto. A leitura de disco só acontece dentro de
/// `resolve()`, chamada uma única vez, no clique.
#[derive(Clone, Copy)]
enum MenuAction {
    /// RFC-002, regra 4: "Nova Expedição" -> save vazio; "Continuar" ->
    /// save carregado do disco (ou vazio, se ausente/corrompido).
    Overworld { fresh: bool },
    Grimoire,
    StyleGuide,
    Quit,
}

impl MenuAction {
    fn resolve(self) -> Transition {
        match self {
            MenuAction::Overworld { fresh } => {
                let save = if fresh { SaveData::default() } else { SaveData::load() };
                Transition::GoToOverworld { save: Box::new(save) }
            }
            MenuAction::Grimoire => Transition::GoToGrimoire,
            MenuAction::StyleGuide => Transition::GoToStyleGuide,
            MenuAction::Quit => Transition::Quit,
        }
    }
}

struct MenuItem {
    key: &'static str,
    label: &'static str,
    action: Option<MenuAction>,
}

pub struct MenuScene {
    hovered: Option<usize>,
}

impl MenuScene {
    pub fn new(_assets: &Assets) -> Self {
        MenuScene { hovered: None }
    }

    fn items() -> Vec<MenuItem> {
        vec![
            MenuItem { key: "01", label: "NOVA EXPEDICAO", action: Some(MenuAction::Overworld { fresh: true }) },
            MenuItem { key: "02", label: "CONTINUAR", action: Some(MenuAction::Overworld { fresh: false }) },
            MenuItem { key: "03", label: "GRIMORIO", action: Some(MenuAction::Grimoire) },
            MenuItem { key: "04", label: "GUIA DE ESTILO", action: Some(MenuAction::StyleGuide) },
            MenuItem { key: "05", label: "SAIR DA PIRAMIDE", action: Some(MenuAction::Quit) },
        ]
    }

    fn item_rect(index: usize) -> Rect {
        let y = 380.0 + index as f32 * 58.0;
        Rect::new(60.0, y, LEFT_W - 120.0, 48.0)
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = mouse_position().into();
        let items = Self::items();
        self.hovered = None;
        for (i, _item) in items.iter().enumerate() {
            if Self::item_rect(i).contains(mouse) {
                self.hovered = Some(i);
            }
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(i) = self.hovered {
                let mut items = Self::items();
                return items.remove(i).action.map(MenuAction::resolve);
            }
        }
        None
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);

        // key art a direita, com fade pra esquerda
        draw_texture_ex(
            &assets.bg_menu,
            LEFT_W,
            0.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(WIDTH - LEFT_W, HEIGHT)), ..Default::default() },
        );
        for i in 0..40 {
            let t = i as f32 / 40.0;
            let alpha = 1.0 - t;
            draw_rectangle(LEFT_W + i as f32 * 3.0, 0.0, 3.0, HEIGHT, Color::new(0.08, 0.05, 0.04, alpha * 0.9));
        }

        // painel esquerdo
        draw_rectangle(0.0, 0.0, LEFT_W, HEIGHT, theme::PEDRA);
        draw_rectangle(LEFT_W - 4.0, 0.0, 4.0, HEIGHT, theme::OURO);

        draw_text_ex(
            "EXPEDICAO * SCRIPT COMBAT",
            60.0,
            110.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::ESCARAVELHO, ..Default::default() },
        );
        draw_text_ex("PII", 60.0, 190.0, TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_XL, color: theme::PAPIRO, ..Default::default() });
        draw_text_ex("RAMID", 60.0, 250.0, TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_XL, color: theme::PAPIRO, ..Default::default() });

        draw_rectangle(60.0, 275.0, 140.0, 5.0, theme::OURO);

        let desc = "Escreva o roteiro do seu turno. A piramide executa linha por linha - e nao perdoa parenteses abertos.";
        let mut y = 310.0;
        for line in wrap_text(desc, 46) {
            draw_text_ex(&line, 60.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::POEIRA, ..Default::default() });
            y += 24.0;
        }

        let items = Self::items();
        for (i, item) in items.iter().enumerate() {
            let r = Self::item_rect(i);
            let hovered = self.hovered == Some(i);
            if hovered {
                draw_rectangle(r.x, r.y, r.w, r.h, theme::TIJOLO);
                draw_rectangle(r.x, r.y, 6.0, r.h, theme::OURO);
            } else {
                draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            }
            draw_text_ex(
                item.key,
                r.x + 16.0,
                r.y + 31.0,
                TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::AREIA_ESCURA, ..Default::default() },
            );
            let color = if hovered { theme::OURO } else { theme::PAPIRO };
            draw_text_ex(item.label, r.x + 60.0, r.y + 31.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color, ..Default::default() });
        }

        draw_text_ex(
            "v0.5.0 - BUILD RUST",
            60.0,
            HEIGHT - 24.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::AREIA_ESCURA, ..Default::default() },
        );
    }
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > max_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

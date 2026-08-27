//! Introdução narrativa da campanha (RFC-023, regras 4-8). Cena única,
//! sem motor de diálogo genérico — 5 painéis fixos que avançam por
//! clique/tecla, nunca por tempo, e podem ser pulados a qualquer momento
//! com `ESC` (ou o botão "PULAR [ESC]", sempre visível). É alcançada só
//! por "NOVA EXPEDICAO" (`MenuAction::Phase { fresh: true }` em
//! `scenes/menu.rs`); "CONTINUAR" nunca passa por aqui.
//!
//! Fica de propósito fora do `pauseable` de `main.rs`: `ESC` já significa
//! "pular" nesta cena, e reaproveitar a mesma tecla pra abrir o menu de
//! pausa seria a mesma tecla brigando com duas funções (RFC-023, tabela
//! de riscos).

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::inventory::SaveData;
use crate::scenes::Transition;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

/// RFC-023 regra 5 — 5 painéis, 2 linhas cada, avanço por clique/tecla,
/// nunca por tempo. Sem acento, seguindo o padrão de `monsters/data.rs`
/// e `scenes/menu.rs` — nenhum outro literal do jogo liga acentuação
/// hoje, e o precedente pesa mais que a leitura ligeiramente mais fácil
/// da versão acentuada (texto do `[[storyteller]]`, RFC-023-entrega
/// dele traz a variante acentuada como alternativa registrada).
const INTRO_PANELS: &[[&str; 2]] = &[
    [
        "Voce entrou sozinho na piramide, como todo explorador entra.",
        "Sem exercito, sem bencao - so voce, e a areia se fechando atras.",
    ],
    [
        "Isto nao e uma tumba com guardas. E um tribunal de pedra.",
        "Os Escribas-Arquitetos gravaram uma lei que nunca dorme: a Sentenca Eterna.",
    ],
    [
        "Golpe de espada contra pedra nao e uma sentenca valida aqui.",
        "So o que e dito como instrucao formal e julgado - e conta.",
    ],
    [
        "Forca bruta, sem a palavra certa, so arranha de raspao.",
        "Julgamento pleno so sai da sentenca que nomeia a fraqueza certa.",
    ],
    [
        "Sob a piramide arde uma brasa enterrada: o folego de cada turno.",
        "Esse folego e seu e do guardiao. Escreva antes que ele acabe.",
    ],
];

/// RFC-023 regra 6 — rótulo do botão de pular, mesmo padrão
/// maiúsculo/sem acento dos itens de `menu.rs`.
const SKIP_LABEL: &str = "PULAR [ESC]";

pub struct IntroScene {
    save: Box<SaveData>,
    panel: usize,
    btn_skip: Button,
}

impl IntroScene {
    pub fn new(save: Box<SaveData>) -> Self {
        IntroScene {
            save,
            panel: 0,
            btn_skip: Button::new(SKIP_LABEL, vec2(WIDTH - 220.0, HEIGHT - 76.0), vec2(160.0, 48.0), ButtonStyle::Ghost, theme::TITLE_SM),
        }
    }

    fn finish(&mut self) -> Transition {
        // RFC-023 regra 7: terminar (ou pular) dispara `GoToPhase` com o
        // mesmo `save` que já vinha do menu -- a introdução é puramente
        // narrativa, não altera nenhum estado de progressão.
        Transition::GoToPhase { save: std::mem::replace(&mut self.save, Box::new(SaveData::default())) }
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = mouse_position().into();
        self.btn_skip.update_hover(mouse);

        // RFC-023 regra 6: pulável a qualquer momento -- ESC ou o botão.
        if is_key_pressed(KeyCode::Escape) || self.btn_skip.clicked(mouse) {
            return Some(self.finish());
        }

        // RFC-023 regra 5: avanço por clique/tecla, nunca por tempo. Só
        // um clique fora do botão de pular ou uma tecla qualquer (exceto
        // ESC, já tratado acima) avança o painel -- não é o mesmo clique
        // que ativa o botão de pular, então checamos hover antes.
        let advance = (is_mouse_button_pressed(MouseButton::Left) && !self.btn_skip.hovered) || any_key_pressed_except_escape();

        if advance {
            if self.panel + 1 >= INTRO_PANELS.len() {
                return Some(self.finish());
            }
            self.panel += 1;
        }

        None
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);
        draw_texture_ex(
            &assets.bg_dungeon,
            0.0,
            0.0,
            Color::new(1.0, 1.0, 1.0, 0.28),
            DrawTextureParams { dest_size: Some(vec2(WIDTH, HEIGHT)), ..Default::default() },
        );

        let card_w = 760.0;
        let card_h = 300.0;
        let card_x = (WIDTH - card_w) / 2.0;
        let card_y = HEIGHT / 2.0 - card_h / 2.0;

        // painel 1: retrato do jogador, único momento fora de combate em
        // que ele aparece (nota do storyteller/designer em
        // RFC-023-entrega-storyteller.md) -- reaproveita o asset
        // existente, nenhum PNG novo.
        if self.panel == 0 {
            let portrait_size = 128.0;
            draw_texture_ex(
                &assets.portrait_player,
                card_x + (card_w - portrait_size) / 2.0,
                card_y - portrait_size - 24.0,
                WHITE,
                DrawTextureParams { dest_size: Some(vec2(portrait_size, portrait_size)), ..Default::default() },
            );
        }

        draw_rectangle(card_x + 8.0, card_y + 8.0, card_w, card_h, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_rectangle(card_x, card_y, card_w, card_h, theme::PEDRA);
        draw_rectangle_lines(card_x, card_y, card_w, card_h, 4.0, theme::OURO);

        let [line1, line2] = INTRO_PANELS[self.panel];
        let mut y = card_y + 110.0;
        for line in wrap_text(line1, 58) {
            draw_text_ex(&line, card_x + 44.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::PAPIRO, ..Default::default() });
            y += 26.0;
        }
        y += 12.0;
        for line in wrap_text(line2, 58) {
            draw_text_ex(&line, card_x + 44.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::PAPIRO, ..Default::default() });
            y += 26.0;
        }

        let progress = format!("{}/{}", self.panel + 1, INTRO_PANELS.len());
        draw_text_ex(
            &progress,
            card_x + card_w - 70.0,
            card_y + card_h - 20.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );

        self.btn_skip.draw(&assets.font_title);
    }
}

/// Verdadeiro se qualquer tecla (exceto `ESC`, que já é tratada à parte
/// como "pular") foi pressionada neste frame -- é o gatilho de "avançar
/// por tecla" da regra 5, sem amarrar a um código específico (Enter,
/// espaço, qualquer letra... todos avançam).
fn any_key_pressed_except_escape() -> bool {
    get_last_key_pressed().is_some_and(|k| k != KeyCode::Escape)
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

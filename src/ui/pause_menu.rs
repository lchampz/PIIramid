//! Sobreposição de pausa (RFC-019). Vive fora de `OverworldScene` de
//! propósito -- a RFC pede pausa central no loop principal (`main.rs`),
//! cobrindo overworld e duelo de graça, sem duplicar a lógica de `ESC`
//! dentro de cada cena. Este módulo só *compõe* `Button`/`theme` já
//! existentes (RFC-019, não-objetivo 3 / alocação do designer): nenhuma
//! paleta nova, nenhum componente novo do zero.
//!
//! `ESC` (alternar pausa) é responsabilidade exclusiva de `main.rs` --
//! este módulo só lê clique nos 2 botões, para não ler `ESC` duas vezes
//! no mesmo frame (o toggle central já despausa; se `update` também
//! reagisse a `ESC` aqui, o mesmo frame despausaria e re-pausaria).

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::screen_scale::virtual_mouse_position;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

const PANEL_W: f32 = 480.0;
const PANEL_H: f32 = 260.0;
const BTN_W: f32 = 360.0;
const BTN_H: f32 = 52.0;

/// O que o jogador escolheu no painel de pausa.
pub enum PauseAction {
    /// "CONTINUAR" -- `main.rs` só zera o flag de pausa, nenhum estado
    /// de expedição é tocado (RFC-019, regra 5).
    Continue,
    /// "VOLTAR AO MENU PRINCIPAL" -- `main.rs` dispara
    /// `Transition::GoToMenu` (já existe, sem mudança de comportamento
    /// de save: RFC-002 regra 4 continua valendo, abandona sem salvar).
    GoToMenu,
}

pub struct PauseOverlay {
    btn_continue: Button,
    btn_menu: Button,
    /// Achado #7 da auditoria de QoL: um único clique em "VOLTAR AO MENU
    /// PRINCIPAL" descartava o duelo em andamento sem aviso, um clique de
    /// mais no painel que fica sobreposto perto de "CONTINUAR". `true`
    /// depois do primeiro clique -- o botão muda de rótulo/estilo pra
    /// "CONFIRMAR" (regra de dois cliques, sem modal novo, mesmo padrão de
    /// card de perigo que `duel.rs` já usa em erro de sintaxe). Reseta ao
    /// clicar CONTINUAR ou ao fechar/reabrir a pausa (nova `PauseOverlay`).
    confirming_menu: bool,
}

impl PauseOverlay {
    pub fn new() -> Self {
        let panel_x = (WIDTH - PANEL_W) / 2.0;
        let panel_y = (HEIGHT - PANEL_H) / 2.0;
        let btn_x = panel_x + (PANEL_W - BTN_W) / 2.0;
        let btn_continue_y = panel_y + 120.0;
        let btn_menu_y = btn_continue_y + BTN_H + 16.0;
        PauseOverlay {
            btn_continue: Button::new("CONTINUAR", vec2(btn_x, btn_continue_y), vec2(BTN_W, BTN_H), ButtonStyle::Primary, theme::TITLE_SM),
            btn_menu: Button::new("VOLTAR AO MENU PRINCIPAL", vec2(btn_x, btn_menu_y), vec2(BTN_W, BTN_H), ButtonStyle::Secondary, theme::TITLE_SM),
            confirming_menu: false,
        }
    }

    /// Achado #7: reseta a confirmação armada de "VOLTAR AO MENU" -- chamado
    /// por `update` ao clicar CONTINUAR, e por `main.rs` sempre que `ESC`
    /// alterna a pausa (nos dois sentidos), pra nunca deixar o botão
    /// "pré-armado" numa reabertura futura do painel.
    pub fn reset_confirm(&mut self) {
        if self.confirming_menu {
            self.confirming_menu = false;
            self.btn_menu.label = "VOLTAR AO MENU PRINCIPAL".to_string();
            self.btn_menu.style = ButtonStyle::Secondary;
        }
    }

    /// Só lê os 2 botões -- nenhum estado de jogo é avançado aqui (é
    /// exatamente por isso que pausar congela de verdade: enquanto isto
    /// substitui `OverworldScene::update()` no loop principal, nada além
    /// de hover/clique dos botões acontece).
    pub fn update(&mut self) -> Option<PauseAction> {
        let mouse: Vec2 = virtual_mouse_position().into();
        self.btn_continue.update_hover(mouse);
        self.btn_menu.update_hover(mouse);

        if self.btn_continue.clicked(mouse) {
            self.reset_confirm();
            return Some(PauseAction::Continue);
        }
        if self.btn_menu.clicked(mouse) {
            if self.confirming_menu {
                return Some(PauseAction::GoToMenu);
            }
            self.confirming_menu = true;
            // mesmo teto de comprimento que "VOLTAR AO MENU PRINCIPAL"
            // (25 caracteres) já prova caber em `BTN_W`/`TITLE_SM`.
            self.btn_menu.label = "CONFIRMAR? PERDE O TURNO".to_string();
            self.btn_menu.style = ButtonStyle::Danger;
        }
        None
    }

    pub fn draw(&self, assets: &Assets) {
        // Véu escuro sobre a tela inteira -- a cena congelada (RFC-019,
        // regra 3) continua visível por baixo, só escurecida.
        draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6));

        let panel_x = (WIDTH - PANEL_W) / 2.0;
        let panel_y = (HEIGHT - PANEL_H) / 2.0;

        // Mesma profundidade de sombra deslocada que `gameover.rs` usa
        // no card de fim de combate (offset 8px, preto a 55%).
        draw_rectangle(panel_x + 8.0, panel_y + 8.0, PANEL_W, PANEL_H, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_rectangle(panel_x, panel_y, PANEL_W, PANEL_H, theme::PEDRA);
        draw_rectangle_lines(panel_x, panel_y, PANEL_W, PANEL_H, 4.0, theme::OURO);

        let title = "PAUSADO";
        let dims = measure_text(title, Some(&assets.font_title), theme::TITLE_LG, 1.0);
        draw_text_ex(
            title,
            panel_x + (PANEL_W - dims.width) / 2.0,
            panel_y + 56.0,
            TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_LG, color: theme::OURO, ..Default::default() },
        );

        self.btn_continue.draw(&assets.font_title);
        self.btn_menu.draw(&assets.font_title);
    }
}

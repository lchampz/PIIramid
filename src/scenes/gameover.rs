//! Tela de fim de combate — porta de `PIIramid Layout.dc.html` (tela
//! "Fim de combate"): card central com grid de estatísticas reais do
//! duelo e botões de reiniciar/menu.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::inventory::SaveData;
use crate::scenes::Transition;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

pub struct GameOverScene {
    won: bool,
    turns: u32,
    player_hp: i32,
    /// RFC-028, regra 4: só populado quando `won == true` e a vitória veio
    /// de `PhaseScene` derrotando o 7o monstro — mesmo texto de feedback
    /// que `MenuScene` mostra nas vitórias parciais, aqui porque a vitória
    /// final pula o menu e vai direto para `GoToGameOver`
    /// (`scenes/phase.rs`).
    last_drop: Option<String>,
    btn_restart: Button,
    btn_menu: Button,
}

impl GameOverScene {
    pub fn new(won: bool, turns: u32, player_hp: i32, last_drop: Option<String>) -> Self {
        let card_w = 760.0;
        let card_x = (WIDTH - card_w) / 2.0;
        let buttons_y = HEIGHT / 2.0 + 170.0;
        GameOverScene {
            won,
            turns,
            player_hp,
            last_drop,
            btn_restart: Button::new(
                if won { "PROXIMA CAMARA" } else { "TENTAR DE NOVO" },
                vec2(card_x + 40.0, buttons_y),
                vec2(280.0, 56.0),
                ButtonStyle::Primary,
                theme::TITLE_SM,
            ),
            btn_menu: Button::new("MENU", vec2(card_x + 340.0, buttons_y), vec2(160.0, 56.0), ButtonStyle::Secondary, theme::TITLE_SM),
        }
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = mouse_position().into();
        self.btn_restart.update_hover(mouse);
        self.btn_menu.update_hover(mouse);

        if self.btn_restart.clicked(mouse) {
            // RFC-002: o save já foi persistido antes de chegar aqui
            // (vitória ou derrota) -- recarregar do disco é o que faz
            // "tentar de novo"/"proxima camara" manter inventario e
            // scripts em vez de voltar para vazio.
            //
            // B-008: até aqui isto devolvia `GoToOverworld`, que reabria o
            // mapa livre com os 7 monstros soltos -- ou seja, qualquer
            // derrota jogava o jogador *fora* da progressão linear da
            // RFC-005, que é o fluxo padrão do jogo desde então. Como
            // `save.current_phase` é a fonte de verdade da fase atual,
            // `GoToPhase` reconstrói exatamente a fase em que o jogador
            // estava (derrota) ou a próxima (vitória, já incrementada e
            // salva por `PhaseScene`).
            return Some(Transition::GoToPhase { save: Box::new(SaveData::load()) });
        }
        if self.btn_menu.clicked(mouse) {
            return Some(Transition::GoToMenu { last_drop: None });
        }
        if is_key_pressed(KeyCode::Escape) {
            return Some(Transition::Quit);
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
        let card_h = 420.0;
        let card_x = (WIDTH - card_w) / 2.0;
        let card_y = HEIGHT / 2.0 - card_h / 2.0 - 40.0;
        let accent = if self.won { theme::OURO } else { theme::SANGUE };

        draw_rectangle(card_x + 8.0, card_y + 8.0, card_w, card_h, Color::new(0.0, 0.0, 0.0, 0.55));
        draw_rectangle(card_x, card_y, card_w, card_h, theme::PEDRA);
        draw_rectangle_lines(card_x, card_y, card_w, card_h, 4.0, accent);

        let subtitle = format!("TURNO {:02}", self.turns);
        draw_text_ex(&subtitle, card_x + 44.0, card_y + 46.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() });

        // achado #8 da auditoria de QoL: apos a RFC-005, `won: true` so
        // chega aqui quando `current_phase >= PHASES.len()` -- vitoria
        // intermediaria vai direto pro Menu (`phase.rs`), sem passar por
        // esta tela. O texto antigo ("o corredor segue adiante") falava de
        // uma vitoria no meio do caminho que nao existe mais -- essa tela
        // e sempre a Sentenca Eterna encerrada de verdade, fim da piramide.
        let title = if self.won { "SENTENCA CUMPRIDA" } else { "VOCE CAIU" };
        draw_text_ex(title, card_x + 44.0, card_y + 100.0, TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_LG, color: accent, ..Default::default() });

        let flavor = if self.won {
            "O ultimo guardiao desmorona. A Sentenca Eterna reconhece sua escrita - a piramide se abre."
        } else {
            "Seu roteiro travou no ultimo ciclo. A piramide guarda seus papiros - e seu corpo."
        };
        let mut y = card_y + 140.0;
        for line in wrap_text(flavor, 58) {
            draw_text_ex(&line, card_x + 44.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::PAPIRO, ..Default::default() });
            y += 24.0;
        }

        // RFC-028, regra 4: feedback do despojo do último monstro (só
        // presente em vitórias, ver doc comment de `last_drop`) -- uma
        // linha extra logo abaixo do texto de sabor, sem competir com a
        // grade de estatísticas abaixo.
        // RFC-029: `last_drop` pode ter uma segunda linha (separada por
        // `\n`) com a nota da Grade de Eficiência do último monstro —
        // mesma linha extra que `MenuScene` desenha nas vitórias parciais.
        if let Some(drop) = &self.last_drop {
            let mut dy = y + 6.0;
            for line in drop.split('\n') {
                draw_text_ex(line, card_x + 44.0, dy, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::OURO, ..Default::default() });
                dy += 22.0;
            }
        }

        let stats: [(&str, String); 3] = [
            ("TURNOS", format!("{:02}", self.turns)),
            ("VIDA RESTANTE", format!("{}", self.player_hp.max(0))),
            ("RESULTADO", if self.won { "VITORIA".to_string() } else { "DERROTA".to_string() }),
        ];
        let stat_w = (card_w - 88.0 - 32.0) / 3.0;
        for (i, (label, value)) in stats.iter().enumerate() {
            let sx = card_x + 44.0 + i as f32 * (stat_w + 16.0);
            let sy = card_y + 220.0;
            draw_rectangle(sx, sy, stat_w, 80.0, theme::TUMBA);
            draw_rectangle_lines(sx, sy, stat_w, 80.0, 2.0, theme::TIJOLO);
            draw_text_ex(label, sx + 12.0, sy + 24.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::AREIA_ESCURA, ..Default::default() });
            draw_text_ex(value, sx + 12.0, sy + 58.0, TextParams { font: Some(&assets.font_title), font_size: 20, color: theme::PAPIRO, ..Default::default() });
        }

        self.btn_restart.draw(&assets.font_title);
        self.btn_menu.draw(&assets.font_title);
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

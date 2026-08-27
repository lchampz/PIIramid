//! Tela de menu — porta de `PIIramid Layout.dc.html` (tela "Menu"): duas
//! colunas, título+descrição+lista de navegação à esquerda, key art com
//! gradiente à direita. Substitui o menu centrado anterior.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::inventory::SaveData;
use crate::monsters::PHASES;
use crate::scenes::Transition;
use crate::ui::theme;

const LEFT_W: f32 = 620.0;

// RFC-023 regras 1-3: animacao de entrada do menu. Deterministica por tempo
// (mesmo padrao de `get_time()` do idle bob/blink de carga em `duel.rs`),
// nunca bloqueia input (regra 2 -- ver `MenuScene::entry_elapsed`), e roda
// toda vez que `MenuScene::new` e chamado, inclusive ao voltar de uma fase
// (regra 3).
const ENTRY_DURATION: f32 = 0.8;
const ENTRY_LOGO_DUR: f32 = 0.35;
const ENTRY_KEYART_DUR: f32 = 0.5;
const ENTRY_ITEM_START: f32 = 0.15;
const ENTRY_ITEM_STAGGER: f32 = 0.05;
const ENTRY_ITEM_DUR: f32 = 0.25;

/// Progresso 0.0-1.0 de um elemento que comeca em `start` e dura `dur`
/// segundos dentro do tempo decorrido `t` da animacao de entrada.
fn entry_ease(t: f32, start: f32, dur: f32) -> f32 {
    ((t - start) / dur).clamp(0.0, 1.0)
}

/// Multiplica o alpha de uma cor do tema pelo progresso de entrada, sem
/// introduzir cor nova -- só reduz opacidade das cores existentes.
fn fade(color: Color, progress: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * progress)
}

/// Ação de cada item do menu, mais leve que `Transition` (que agora carrega
/// `SaveData` — RFC-002). `items()` roda todo frame só pra detectar hover;
/// construir `Transition::GoToPhase`/`GoToOverworld` ali significaria
/// ler/zerar o save a cada frame com o menu aberto. A leitura de disco só
/// acontece dentro de `resolve()`, chamada uma única vez, no clique.
#[derive(Clone, Copy)]
enum MenuAction {
    /// RFC-002, regra 4 (agora endereçado a `GoToPhase` — RFC-005 regra 6):
    /// "Nova Expedição" -> save vazio; "Continuar" -> save carregado do
    /// disco (ou vazio, se ausente/corrompido).
    Phase { fresh: bool },
    /// RFC-005 regra 6: mapa livre, só em build debug -- `GoToOverworld`
    /// continua intocado, só deixa de ser o caminho padrão do menu.
    DebugOverworld,
    Grimoire,
    StyleGuide,
    Quit,
}

impl MenuAction {
    fn resolve(self) -> Transition {
        match self {
            MenuAction::Phase { fresh } => {
                let save = if fresh { SaveData::default() } else { SaveData::load() };
                // RFC-023 regra 4: só "Nova Expedicao" (`fresh: true`) passa
                // pela introducao narrativa antes do primeiro duelo;
                // "Continuar" mantem o caminho direto para `GoToPhase` --
                // ninguem deve reler a intro num segundo playthrough.
                if fresh {
                    Transition::GoToIntro { save: Box::new(save) }
                } else {
                    Transition::GoToPhase { save: Box::new(save) }
                }
            }
            MenuAction::DebugOverworld => Transition::GoToOverworld { save: Box::new(SaveData::load()) },
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
    /// RFC-026 regra 3: `SaveData::load()` acontece uma única vez aqui, no
    /// mesmo padrão que `GrimoireScene::new` já paga (`grimoire.rs:44`) —
    /// não em `update()`/`draw()`, que rodam a cada frame. `next_room` é
    /// `None` quando `current_phase >= PHASES.len()` (pirâmide concluída);
    /// a mensagem "PIRAMIDE CONCLUIDA" nasce daqui, não de uma checagem
    /// espalhada em `draw()`.
    current_phase: usize,
    next_room: Option<&'static str>,
    /// RFC-023 regra 1/3: instante (`get_time()`) em que esta `MenuScene` foi
    /// construida -- referencia pra animacao de entrada. Recalculado toda
    /// vez que `new()` roda, entao voltar ao menu depois de uma fase repete
    /// a animacao (comportamento default pedido pela regra 3).
    opened_at: f64,
    /// RFC-023 regra 2: vira `true` no primeiro input (tecla ou clique)
    /// detectado em `update()` enquanto a entrada ainda anima. Uma vez
    /// `true`, `entry_elapsed()` passa a devolver `ENTRY_DURATION` direto
    /// (estado final), sem interpolar mais -- a animacao nunca "engole"
    /// esse mesmo input, que continua sendo processado normalmente no
    /// resto de `update()`.
    entry_skipped: bool,
    /// RFC-028, regra 4: texto de feedback do despojo de vitória
    /// (`inventory::apply_phase_victory_drop`), recebido via
    /// `Transition::GoToMenu` só quando a fase que acabou de terminar foi
    /// uma vitória real — `None` em qualquer outro caminho até o menu
    /// (derrota, fuga, VOLTAR do Grimório/Guia de Estilo, ESC do menu de
    /// pausa). Não é persistido em `SaveData`: é só o texto de uma única
    /// tela, no mesmo espírito efêmero do `log` de `DuelScene`.
    last_drop: Option<String>,
}

impl MenuScene {
    pub fn new(_assets: &Assets, last_drop: Option<String>) -> Self {
        let save = SaveData::load();
        let next_room = PHASES.get(save.current_phase).map(|(_, spec_fn)| spec_fn().room);
        MenuScene {
            hovered: None,
            current_phase: save.current_phase,
            next_room,
            opened_at: get_time(),
            entry_skipped: false,
            last_drop,
        }
    }

    /// Segundos decorridos desde que o menu foi construido, para fins da
    /// animacao de entrada -- nunca mais que `ENTRY_DURATION`. Ver
    /// `entry_skipped` pra regra 2 (input salta pro estado final).
    fn entry_elapsed(&self) -> f32 {
        if self.entry_skipped {
            ENTRY_DURATION
        } else {
            ((get_time() - self.opened_at) as f32).min(ENTRY_DURATION)
        }
    }

    fn items() -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem { key: "01", label: "NOVA EXPEDICAO", action: Some(MenuAction::Phase { fresh: true }) },
            MenuItem { key: "02", label: "CONTINUAR", action: Some(MenuAction::Phase { fresh: false }) },
            MenuItem { key: "03", label: "GRIMORIO", action: Some(MenuAction::Grimoire) },
            MenuItem { key: "04", label: "GUIA DE ESTILO", action: Some(MenuAction::StyleGuide) },
        ];
        // RFC-005 regra 6: mapa livre acessível só em build debug, pra
        // testar uma câmara específica sem jogar a sequência inteira. Não
        // aparece em `cargo build --release` -- `#[cfg(debug_assertions)]`
        // é o mecanismo padrão do Rust pra isso, nenhuma flag própria.
        #[cfg(debug_assertions)]
        items.push(MenuItem { key: "05", label: "MAPA (DEBUG)", action: Some(MenuAction::DebugOverworld) });
        let quit_key = if cfg!(debug_assertions) { "06" } else { "05" };
        items.push(MenuItem { key: quit_key, label: "SAIR DA PIRAMIDE", action: Some(MenuAction::Quit) });
        items
    }

    fn item_rect(index: usize) -> Rect {
        let y = 380.0 + index as f32 * 58.0;
        Rect::new(60.0, y, LEFT_W - 120.0, 48.0)
    }

    pub fn update(&mut self) -> Option<Transition> {
        // RFC-023 regra 2: qualquer tecla ou clique salta a animacao de
        // entrada pro estado final -- checado antes de qualquer outra coisa,
        // mas sem "consumir" o input: `is_mouse_button_pressed`/
        // `get_last_key_pressed` continuam sendo lidos normalmente logo
        // abaixo neste mesmo frame, e o hover/clique dos itens usa sempre
        // `item_rect()` (posicao final, nunca a posicao animada), entao o
        // clique que disparou o salto tambem já acerta o item certo.
        if !self.entry_skipped
            && (is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right)
                || get_last_key_pressed().is_some())
        {
            self.entry_skipped = true;
        }

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

        // RFC-023 regra 1: progresso da animacao de entrada, 0.0-1.0 por
        // elemento. `entry_elapsed()` ja devolve o valor final direto se a
        // regra 2 (input durante a entrada) ja disparou.
        let t = self.entry_elapsed();
        let keyart_progress = entry_ease(t, 0.0, ENTRY_KEYART_DUR);
        let logo_progress = entry_ease(t, 0.0, ENTRY_LOGO_DUR);

        // key art a direita, com fade pra esquerda -- fade de entrada
        // (regra 1) multiplica o alpha em cima do gradiente que ja existia.
        draw_texture_ex(
            &assets.bg_menu,
            LEFT_W,
            0.0,
            Color::new(1.0, 1.0, 1.0, keyart_progress),
            DrawTextureParams { dest_size: Some(vec2(WIDTH - LEFT_W, HEIGHT)), ..Default::default() },
        );
        for i in 0..40 {
            let g = i as f32 / 40.0;
            let alpha = 1.0 - g;
            draw_rectangle(LEFT_W + i as f32 * 3.0, 0.0, 3.0, HEIGHT, Color::new(0.08, 0.05, 0.04, alpha * 0.9 * keyart_progress));
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
        // RFC-023 regra 1: logo "desce" com fade -- comeca deslocado pra
        // cima e um pouco transparente, chega na posicao/opacidade final em
        // `ENTRY_LOGO_DUR` segundos.
        let logo_offset_y = (1.0 - logo_progress) * -24.0;
        let logo_color = Color::new(theme::PAPIRO.r, theme::PAPIRO.g, theme::PAPIRO.b, logo_progress);
        draw_text_ex(
            "PII",
            60.0,
            190.0 + logo_offset_y,
            TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_XL, color: logo_color, ..Default::default() },
        );
        draw_text_ex(
            "RAMID",
            60.0,
            250.0 + logo_offset_y,
            TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_XL, color: logo_color, ..Default::default() },
        );

        draw_rectangle(60.0, 275.0 + logo_offset_y, 140.0, 5.0, Color::new(theme::OURO.r, theme::OURO.g, theme::OURO.b, logo_progress));

        let desc = "Escreva o roteiro do seu turno. A piramide executa linha por linha - e nao perdoa parenteses abertos.";
        let mut y = 310.0;
        for line in wrap_text(desc, 46) {
            draw_text_ex(&line, 60.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::POEIRA, ..Default::default() });
            y += 24.0;
        }

        // RFC-026 regra 3: linha de status entre a descrição e a lista de
        // itens -- "PROGRESSO: FASE N/7 - nome da proxima camara", ou
        // "PIRAMIDE CONCLUIDA" quando não há próxima fase. ESCARAVELHO só
        // no número da fase (papel de "informação" já consolidado pela
        // auditoria de identidade visual), o resto em POEIRA como o resto
        // do texto de apoio desta tela.
        y += 8.0;
        draw_text_ex("PROGRESSO: ", 60.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() });
        let prefix_dims = measure_text("PROGRESSO: ", Some(&assets.font_body), theme::BODY_MD, 1.0);
        match self.next_room {
            Some(room) => {
                let phase_label = format!("FASE {}/{}", self.current_phase + 1, PHASES.len());
                draw_text_ex(&phase_label, 60.0 + prefix_dims.width, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::ESCARAVELHO, ..Default::default() });
                let phase_dims = measure_text(&phase_label, Some(&assets.font_body), theme::BODY_MD, 1.0);
                draw_text_ex(
                    format!(" - {room}"),
                    60.0 + prefix_dims.width + phase_dims.width,
                    y,
                    TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() },
                );
            }
            None => {
                draw_text_ex(
                    "PIRAMIDE CONCLUIDA",
                    60.0 + prefix_dims.width,
                    y,
                    TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::OURO, ..Default::default() },
                );
            }
        }

        // RFC-028, regra 4: feedback do despojo recebido ao vencer a fase
        // anterior -- só aparece quando `Transition::GoToMenu` chegou com
        // `last_drop: Some(..)` (vitória real), uma linha a mais logo
        // abaixo do "PROGRESSO", sem tela nova.
        if let Some(drop) = &self.last_drop {
            y += 24.0;
            draw_text_ex(drop, 60.0, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::OURO, ..Default::default() });
        }

        let items = Self::items();
        for (i, item) in items.iter().enumerate() {
            // RFC-023 regra 1: itens entram em cascata, um leve atraso entre
            // cada. `r` (posicao final) e o que `update()` usa pra
            // hover/clique -- só o desenho desliza a partir de `draw_x`,
            // nunca a area clicavel (é assim que a regra 2 fica garantida
            // mesmo sem o `entry_skipped`: o clique sempre acerta o item
            // certo, animado ou nao).
            let r = Self::item_rect(i);
            let item_progress = entry_ease(t, ENTRY_ITEM_START + i as f32 * ENTRY_ITEM_STAGGER, ENTRY_ITEM_DUR);
            let draw_x = r.x + (1.0 - item_progress) * 40.0;

            let hovered = self.hovered == Some(i);
            if hovered {
                draw_rectangle(draw_x, r.y, r.w, r.h, fade(theme::TIJOLO, item_progress));
                draw_rectangle(draw_x, r.y, 6.0, r.h, fade(theme::OURO, item_progress));
            } else {
                draw_rectangle(draw_x, r.y, r.w, r.h, fade(theme::PEDRA, item_progress));
            }
            draw_text_ex(
                item.key,
                draw_x + 16.0,
                r.y + 31.0,
                TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: fade(theme::AREIA_ESCURA, item_progress), ..Default::default() },
            );
            let color = if hovered { theme::OURO } else { theme::PAPIRO };
            draw_text_ex(
                item.label,
                draw_x + 60.0,
                r.y + 31.0,
                TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: fade(color, item_progress), ..Default::default() },
            );
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

//! Ponto de entrada: substitui o `switch(count)` de `main.c` por um enum
//! de cena explícito. Cada cena devolve `Option<Transition>`; este loop só
//! troca de cena, sem estado global solto (`sys.menu`/`sys.battle`/...).

mod assets;
mod config;
mod grade;
mod inventory;
mod monsters;
mod scenes;
mod screen_scale;
mod script;
mod ui;
mod world;

use macroquad::prelude::*;

use assets::Assets;
use config::{HEIGHT, NAME, WIDTH};
use scenes::gameover::GameOverScene;
use scenes::grimoire::GrimoireScene;
use scenes::intro::IntroScene;
use scenes::menu::MenuScene;
use scenes::overworld::OverworldScene;
use scenes::phase::PhaseScene;
use scenes::style_guide::StyleGuideScene;
use scenes::Transition;
use ui::pause_menu::{PauseAction, PauseOverlay};

enum Scene {
    Menu(MenuScene),
    Overworld(Box<OverworldScene>),
    /// RFC-005: fluxo padrão de duelo (substitui `Overworld` no menu, que
    /// fica só pro debug). Variante própria, não um caso dentro de
    /// `Overworld` — não há mapa/movimento por baixo pra reaproveitar a
    /// estrutura `duel: Option<(usize, DuelScene)>` de `OverworldScene`.
    Phase(Box<PhaseScene>),
    /// RFC-023: introdução narrativa, só alcançável por "NOVA EXPEDICAO".
    /// Fora de propósito do `pauseable` abaixo -- `ESC` já é "pular" aqui.
    Intro(IntroScene),
    GameOver(GameOverScene),
    Grimoire(Box<GrimoireScene>),
    StyleGuide(StyleGuideScene),
}

fn window_conf() -> Conf {
    Conf {
        window_title: NAME.to_string(),
        window_width: WIDTH as i32,
        window_height: HEIGHT as i32,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let assets = Assets::load().await;

    // RFC-031: canvas virtual de 1280x720 renderizado numa textura offscreen
    // e depois desenhado na janela real, escalado e centralizado com
    // letterbox -- criado uma única vez, fora do loop de frame (regra da
    // squad: sem alocação desnecessária no loop de frame).
    let render_target = screen_scale::make_render_target();
    let virtual_camera = screen_scale::virtual_camera(&render_target);

    let mut scene = Scene::Menu(MenuScene::new(&assets, None));

    // RFC-019: pausa só existe enquanto `scene` é `Scene::Overworld` (que
    // também cobre o duelo -- `DuelScene` vive dentro de `OverworldScene`,
    // não é variante própria de `Scene`). Estado central aqui, não
    // duplicado dentro de `OverworldScene`/`DuelScene` (RFC-019, não-objetivo 3).
    let mut paused = false;
    let mut pause_overlay = PauseOverlay::new();

    loop {
        // RFC-019 original: pausa só existia em `Scene::Overworld`. RFC-005
        // troca o fluxo padrão do jogador por `Scene::Phase` (duelo direto,
        // sem mapa) -- sem estender esta checagem, a pausa continuaria
        // "funcionando" só para quem entra pelo item de menu de debug, e
        // quebraria silenciosamente para todo mundo que joga o fluxo normal
        // (exatamente o risco que a RFC-005 pede pra não deixar acontecer).
        // RFC-023: `Scene::Intro` fica fora de propósito -- `ESC` ali já
        // significa "pular a introdução" (regra 6); se também abrisse o
        // menu de pausa, a mesma tecla brigaria com duas funções no mesmo
        // frame (RFC-023, tabela de riscos).
        let pauseable = matches!(scene, Scene::Overworld(_) | Scene::Phase(_));

        // QA (ALTO-2, auditoria de interação RFC-023/026-030): um overlay
        // modal de `DuelScene` (CARREGAR/RFC-026, ENSAIAR/RFC-027, escolha
        // de função/RFC-030) documenta "ESC fecha/pula" como comportamento
        // próprio -- mas este bloco consumia o mesmo `ESC` pra abrir a pausa
        // antes de `scene.update()` sequer rodar, então o overlay nunca via
        // a tecla e a pausa abria por cima dele, congelado. Só suprime o
        // toggle quando a pausa ainda NÃO está ativa (abrir): com a pausa já
        // ativa, `scene.update()` não roda mesmo (regra 3 da RFC-019), então
        // não há overlay novo se abrindo nesse meio tempo e `ESC` fechar a
        // pausa continua seguro.
        let scene_wants_escape_first = matches!(&scene, Scene::Phase(p) if p.has_modal_overlay_open());

        // RFC-019 regra 2: único lugar que lê `ESC` para pausa -- alterna.
        // `PauseOverlay::update` (abaixo) nunca lê `ESC`, só clique nos
        // botões, senão o mesmo toggle aconteceria duas vezes no mesmo
        // frame (pausa e despausa de volta sem o jogador perceber).
        if pauseable && is_key_pressed(KeyCode::Escape) && (!scene_wants_escape_first || paused) {
            paused = !paused;
            // achado #7: ESC fecha a pausa sem passar por
            // `PauseOverlay::update` (que só reseta a confirmação de
            // "VOLTAR AO MENU" em clique de CONTINUAR) -- sem isto, reabrir
            // a pausa depois de ter armado a confirmação e fechado com ESC
            // deixaria o botão já armado, pulando o aviso na vez seguinte.
            pause_overlay.reset_confirm();
        }

        let transition = if pauseable && paused {
            // RFC-019 regra 3: `OverworldScene::update()` não é chamado
            // enquanto pausado -- é isto que congela o jogo de verdade
            // (nenhum monstro anima, nenhum turno de duelo avança).
            match pause_overlay.update() {
                Some(PauseAction::Continue) => {
                    paused = false;
                    None
                }
                Some(PauseAction::GoToMenu) => Some(Transition::GoToMenu { last_drop: None }),
                None => None,
            }
        } else {
            match &mut scene {
                Scene::Menu(s) => s.update(),
                Scene::Overworld(s) => s.update(),
                Scene::Phase(s) => s.update(),
                Scene::Intro(s) => s.update(),
                Scene::GameOver(s) => s.update(),
                Scene::Grimoire(s) => s.update(),
                Scene::StyleGuide(s) => s.update(),
            }
        };

        if let Some(t) = transition {
            match t {
                Transition::GoToOverworld { save } => scene = Scene::Overworld(Box::new(OverworldScene::new(*save))),
                Transition::GoToPhase { save } => scene = Scene::Phase(Box::new(PhaseScene::new(*save))),
                Transition::GoToIntro { save } => scene = Scene::Intro(IntroScene::new(save)),
                Transition::GoToGameOver { won, turns, player_hp, last_drop } => scene = Scene::GameOver(GameOverScene::new(won, turns, player_hp, last_drop)),
                Transition::GoToMenu { last_drop } => scene = Scene::Menu(MenuScene::new(&assets, last_drop)),
                Transition::GoToGrimoire => scene = Scene::Grimoire(Box::new(GrimoireScene::new())),
                Transition::GoToStyleGuide => scene = Scene::StyleGuide(StyleGuideScene::new()),
                Transition::Quit => break,
            }
            // Toda transição encerra a pausa -- ela só faz sentido dentro
            // da instância de overworld em que foi aberta (RFC-019, regra 1).
            paused = false;
        }

        // RFC-031: todo o desenho deste frame (cenas + overlay de pausa) vai
        // para o render target de 1280x720, não direto pra janela real --
        // é o que permite escalar o resultado inteiro de uma vez só, sem
        // que nenhuma cena precise saber do tamanho real da janela.
        set_camera(&virtual_camera);

        match &scene {
            Scene::Menu(s) => s.draw(&assets),
            Scene::Overworld(s) => s.draw(&assets),
            Scene::Phase(s) => s.draw(&assets),
            Scene::Intro(s) => s.draw(&assets),
            Scene::GameOver(s) => s.draw(&assets),
            Scene::Grimoire(s) => s.draw(&assets),
            Scene::StyleGuide(s) => s.draw(&assets),
        }

        // RFC-019 regra 3: `draw()` da cena continua rodando por baixo do
        // véu -- só o desenho da sobreposição é condicional.
        if pauseable && paused {
            pause_overlay.draw(&assets);
        }

        // Volta a desenhar direto na janela real e faz o blit do canvas
        // virtual escalado e centralizado (letterbox preto nas bordas).
        set_default_camera();
        screen_scale::draw_letterboxed(&render_target);

        next_frame().await;

        if is_quit_requested() {
            break;
        }
    }
}

//! Ponto de entrada: substitui o `switch(count)` de `main.c` por um enum
//! de cena explícito. Cada cena devolve `Option<Transition>`; este loop só
//! troca de cena, sem estado global solto (`sys.menu`/`sys.battle`/...).

mod assets;
mod config;
mod monsters;
mod scenes;
mod script;
mod ui;
mod world;

use macroquad::prelude::*;

use assets::Assets;
use config::{HEIGHT, NAME, WIDTH};
use scenes::gameover::GameOverScene;
use scenes::grimoire::GrimoireScene;
use scenes::menu::MenuScene;
use scenes::overworld::OverworldScene;
use scenes::style_guide::StyleGuideScene;
use scenes::Transition;

enum Scene {
    Menu(MenuScene),
    Overworld(Box<OverworldScene>),
    GameOver(GameOverScene),
    Grimoire(GrimoireScene),
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

    let mut scene = Scene::Menu(MenuScene::new(&assets));

    loop {
        let transition = match &mut scene {
            Scene::Menu(s) => s.update(),
            Scene::Overworld(s) => s.update(),
            Scene::GameOver(s) => s.update(),
            Scene::Grimoire(s) => s.update(),
            Scene::StyleGuide(s) => s.update(),
        };

        if let Some(t) = transition {
            match t {
                Transition::GoToOverworld => scene = Scene::Overworld(Box::new(OverworldScene::new())),
                Transition::GoToGameOver { won, turns, player_hp } => scene = Scene::GameOver(GameOverScene::new(won, turns, player_hp)),
                Transition::GoToMenu => scene = Scene::Menu(MenuScene::new(&assets)),
                Transition::GoToGrimoire => scene = Scene::Grimoire(GrimoireScene::new()),
                Transition::GoToStyleGuide => scene = Scene::StyleGuide(StyleGuideScene::new()),
                Transition::Quit => break,
            }
        }

        match &scene {
            Scene::Menu(s) => s.draw(&assets),
            Scene::Overworld(s) => s.draw(&assets),
            Scene::GameOver(s) => s.draw(&assets),
            Scene::Grimoire(s) => s.draw(&assets),
            Scene::StyleGuide(s) => s.draw(&assets),
        }

        next_frame().await;

        if is_quit_requested() {
            break;
        }
    }
}

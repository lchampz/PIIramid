//! O mapa em que o jogador anda e esbarra nos monstros. Porta de
//! `battle.c` (o nome era enganoso lá também — a luta de verdade é o
//! duelo). Diferente do original, que só tinha uma múmia fixa, aqui há um
//! pequeno elenco — cada um força um algoritmo diferente.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::monsters::data;
use crate::monsters::MonsterState;
use crate::scenes::duel::{DuelOutcome, DuelScene};
use crate::scenes::Transition;
use crate::ui::theme;
use crate::world::entity::{overlaps, Direction, Entity, Kind};
use crate::world::tilemap::TileMap;

struct Foe {
    entity: Entity,
    state: MonsterState,
    defeated: bool,
}

pub struct OverworldScene {
    map: TileMap,
    player: Entity,
    foes: Vec<Foe>,
    duel: Option<(usize, DuelScene)>,
}

impl OverworldScene {
    pub fn new() -> Self {
        let map = TileMap::load("./assets/map.txt").expect("mapa invalido");

        let mut player = Entity::new(Kind::Player, true);
        player.position = vec2(200.0, 300.0);

        let foes = vec![
            spawn(Kind::Mummy, data::mummy(), vec2(450.0, 200.0)),
            spawn(Kind::Zombie, data::zombie(), vec2(650.0, 350.0)),
            spawn(Kind::Beetle, data::beetle(), vec2(300.0, 500.0)),
            spawn(Kind::Sphinx, data::sphinx(), vec2(550.0, 560.0)),
        ];

        OverworldScene { map, player, foes, duel: None }
    }

    pub fn update(&mut self) -> Option<Transition> {
        if let Some((idx, mut duel)) = self.duel.take() {
            let foe = &mut self.foes[idx];
            let outcome = duel.update(&mut self.player, &mut foe.state);
            match outcome {
                Some(DuelOutcome::Won) => {
                    foe.defeated = true;
                    if self.foes.iter().all(|f| f.defeated) {
                        return Some(Transition::GoToGameOver { won: true, turns: duel.turn(), player_hp: self.player.life_points });
                    }
                }
                Some(DuelOutcome::Lost) => {
                    return Some(Transition::GoToGameOver { won: false, turns: duel.turn(), player_hp: self.player.life_points });
                }
                Some(DuelOutcome::Fled) => {}
                None => {
                    self.duel = Some((idx, duel));
                }
            }
            return None;
        }

        self.handle_movement();
        self.player.move_and_collide(&self.map);
        self.player.tick_animation();

        for (idx, foe) in self.foes.iter_mut().enumerate() {
            if foe.defeated {
                continue;
            }
            foe.entity.tick_animation();
            if overlaps(&self.player, &foe.entity) {
                self.duel = Some((idx, DuelScene::new()));
                break;
            }
        }

        None
    }

    fn handle_movement(&mut self) {
        let mut moving = false;
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            self.player.set_moving(Direction::Up);
            moving = true;
        } else if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            self.player.set_moving(Direction::Down);
            moving = true;
        } else if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            self.player.set_moving(Direction::Left);
            moving = true;
        } else if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            self.player.set_moving(Direction::Right);
            moving = true;
        }
        if !moving {
            self.player.stop_moving();
        }
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);
        self.map.draw(&assets.tile_floor, &assets.tile_wall);

        if let Some((idx, duel)) = &self.duel {
            duel.draw(assets, &self.player, &self.foes[*idx].state, self.foes[*idx].entity.kind);
            return;
        }

        self.player.draw(assets.sprite_for(self.player.kind));
        for foe in &self.foes {
            if !foe.defeated {
                foe.entity.draw(assets.sprite_for(foe.entity.kind));
            }
        }

        draw_text_ex(
            "VIDA",
            20.0,
            34.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() },
        );
        draw_rectangle(90.0, 18.0, 200.0, 26.0, theme::TUMBA);
        let ratio = (self.player.life_points.max(0) as f32 / self.player.max_life.max(1) as f32).clamp(0.0, 1.0);
        draw_rectangle(90.0, 18.0, 200.0 * ratio, 26.0, theme::VIDA);
        draw_rectangle_lines(90.0, 18.0, 200.0, 26.0, 2.0, theme::OURO);

        let remaining = self.foes.iter().filter(|f| !f.defeated).count();
        draw_text_ex(
            format!("MONSTROS RESTANTES: {remaining}"),
            20.0,
            70.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() },
        );
    }
}

fn spawn(kind: Kind, spec: crate::monsters::MonsterSpec, position: Vec2) -> Foe {
    let mut entity = Entity::new(kind, false);
    entity.position = position;
    Foe { entity, state: MonsterState::new(spec), defeated: false }
}

//! O mapa em que o jogador anda e esbarra nos monstros. Porta de
//! `battle.c` (o nome era enganoso lá também — a luta de verdade é o
//! duelo). Diferente do original, que só tinha uma múmia fixa, aqui há um
//! pequeno elenco — cada um força um algoritmo diferente.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, SPRITE_FRAME, WIDTH};
use crate::inventory::SaveData;
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
    /// Inventário/scripts do jogador (RFC-002). Vive aqui — não em
    /// `DuelScene` — porque sobrevive a todos os duelos da expedição, não
    /// só ao atual; é emprestado ao duelo em andamento por `update`.
    save: SaveData,
}

impl OverworldScene {
    pub fn new(save: SaveData) -> Self {
        let map = TileMap::load("./assets/map.txt").expect("mapa invalido");

        let mut player = Entity::new(Kind::Player, true);
        player.position = vec2(200.0, 300.0);

        let foes = vec![
            spawn(Kind::Mummy, data::mummy(), vec2(450.0, 200.0)),
            spawn(Kind::Zombie, data::zombie(), vec2(650.0, 350.0)),
            spawn(Kind::Beetle, data::beetle(), vec2(300.0, 500.0)),
            spawn(Kind::Sphinx, data::sphinx(), vec2(550.0, 560.0)),
            // RFC-008: `assets/map.txt` declara 23x40 tiles (32px), sem
            // parede interna -- so a borda tem colisao
            // (world/tilemap.rs::Tile::has_collision) -- mas a area
            // efetivamente alcancavel pelo jogador e menor: o clamp de
            // `Entity::move_and_collide` (world/entity.rs) trava o
            // jogador em x em [0, WIDTH-64] e y em [0, HEIGHT-64], ou
            // seja y <= 656 (WIDTH/HEIGHT = 1280x720, config.rs), bem
            // antes da borda inferior do mapa (linha 39, y=1248). Um
            // Guardiao em y=850, por exemplo, ficaria fora do alcance do
            // jogador -- por isso a posicao abaixo fica dentro de
            // [32, 704]x[32, 656] (piso interior x alcance do jogador),
            // com hitbox 64x64 (SPRITE_FRAME) e margem > 100px dos outros
            // 4 (y entre 200 e 560) e do spawn do jogador (200,300).
            spawn(Kind::Guardiao, data::guardiao(), vec2(150.0, 640.0)),
            // RFC-012: mesma area alcancavel do comentario acima
            // ([32,1184]x[32,656] de piso dentro do clamp de movimento).
            // x=900 fica a >250px de todos os outros 5 spawns em ambos os
            // eixos e a >700px do Guardiao (mesmo y=640), sem tocar a
            // parede da borda inferior (y+64=704 == borda, igual ao
            // Guardiao, que ja funciona nessa mesma linha).
            spawn(Kind::Sentinela, data::sentinela(), vec2(900.0, 640.0)),
            // RFC-017: piso interior sem colisao real (map.collides so
            // olha tiles dentro de cols/rows do mapa 23x40 -- alem disso
            // nao ha parede) e dentro do clamp de movimento
            // ([0, WIDTH-64]x[0, HEIGHT-64] = [0,1216]x[0,656]). x=1100,
            // y=460 fica a >100px de todos os outros 6 spawns nos dois
            // eixos: Mummy(450,200) dx=650, Zombie(650,350) dx=450,
            // Beetle(300,500) dx=800, Sphinx(550,560) dx=550,
            // Guardiao(150,640) dx=950, Sentinela(900,640) dx=200 dy=180,
            // jogador(200,300) dx=900.
            spawn(Kind::Necroguardiao, data::necroguardiao(), vec2(1100.0, 460.0)),
        ];

        OverworldScene { map, player, foes, duel: None, save }
    }

    pub fn update(&mut self) -> Option<Transition> {
        if let Some((idx, mut duel)) = self.duel.take() {
            let foe = &mut self.foes[idx];
            let outcome = duel.update(&mut self.player, &mut foe.state, &mut self.save);
            match outcome {
                Some(DuelOutcome::Won) => {
                    foe.defeated = true;
                    if self.foes.iter().all(|f| f.defeated) {
                        // RFC-002 regra 4: o save é sobrescrito quando o
                        // jogador sai do overworld/duelo de verdade (aqui,
                        // ao vencer a expedição) -- nunca só por navegar o
                        // menu.
                        self.save.save();
                        return Some(Transition::GoToGameOver { won: true, turns: duel.turn(), player_hp: self.player.life_points, last_drop: None });
                    }
                }
                Some(DuelOutcome::Lost) => {
                    self.save.save();
                    return Some(Transition::GoToGameOver { won: false, turns: duel.turn(), player_hp: self.player.life_points, last_drop: None });
                }
                Some(DuelOutcome::Fled) => {
                    // Bug de gameplay achado em playtest gravado: fugir não
                    // afastava o jogador do monstro, então no frame
                    // seguinte `overlaps()` (abaixo, no loop de colisão)
                    // continuava verdadeiro e o mesmo duelo recomeçava na
                    // hora -- "fugir" parecia simplesmente não funcionar.
                    // Empurra o jogador na direção oposta ao monstro por
                    // mais que 2x SPRITE_FRAME (64px), suficiente pra
                    // limpar a sobreposição de AABB mesmo se as duas
                    // hitboxes estivessem exatamente coincidentes.
                    let away = (self.player.position - foe.entity.position).normalize_or_zero();
                    let away = if away.length_squared() < f32::EPSILON { vec2(0.0, -1.0) } else { away };
                    let pushed = self.player.position + away * (SPRITE_FRAME * 2.0);
                    self.player.position = pushed.clamp(vec2(0.0, 0.0), vec2(WIDTH - SPRITE_FRAME, HEIGHT - SPRITE_FRAME));
                }
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
                // mapa livre de debug não tem noção de campanha linear
                // (RFC-005) -- nunca é "a fase final", então a tela de
                // escolha de função compilada (RFC-030) sempre pode aparecer
                // aqui, sem afetar nada real (nada é persistido no fluxo de
                // debug do jeito que a campanha persiste).
                self.duel = Some((idx, DuelScene::new(false)));
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
            // `OverworldScene` (mapa livre de debug) não tem noção de fase
            // linear da pirâmide — `None` faz `DuelScene` omitir "FASE N/7".
            duel.draw(assets, &self.player, &self.foes[*idx].state, self.foes[*idx].entity.kind, &self.save, None);
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

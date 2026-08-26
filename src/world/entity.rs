//! Entidades do overworld (jogador e monstros que perseguem).
//! Substitui `entity.c`/`entity.h`. Ao contrário do original, a colisão
//! contra o mapa usa o retângulo (AABB) da hitbox contra os tiles com
//! colisão, em vez de checar só se a posição está na borda da tela.

use macroquad::prelude::*;

use crate::config::{HEIGHT, SPRITE_FRAME, WIDTH};
use crate::world::tilemap::TileMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Down,
    Up,
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Animation {
    Running,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Player,
    Zombie,
    Mummy,
    Beetle,
    Sphinx,
}

pub struct Entity {
    pub alive: bool,
    pub life_points: i32,
    pub max_life: i32,
    pub speed: f32,

    pub hitbox: Vec2,
    pub position: Vec2,

    pub frame: i32,
    pub direction: Direction,
    pub frame_delay: i32,
    pub count_frame: i32,
    pub animation: Animation,
    pub kind: Kind,

    pub is_moving: bool,
}

impl Entity {
    pub fn new(kind: Kind, is_player: bool) -> Self {
        let hitbox = vec2(SPRITE_FRAME, SPRITE_FRAME);

        Entity {
            alive: true,
            max_life: 100,
            life_points: 100,
            speed: if is_player { 5.0 } else { 2.0 },
            hitbox,
            position: vec2(WIDTH / 2.0 - hitbox.x / 2.0, (HEIGHT / 2.0 - hitbox.y / 2.0) - 40.0),
            frame: 0,
            direction: Direction::Up,
            frame_delay: 8,
            count_frame: 8,
            animation: Animation::Idle,
            kind,
            is_moving: false,
        }
    }

    pub fn set_moving(&mut self, direction: Direction) {
        self.direction = direction;
        self.is_moving = true;
    }

    pub fn stop_moving(&mut self) {
        self.is_moving = false;
    }

    /// Retângulo de colisão atual, no espaço do mundo.
    pub fn rect(&self) -> Rect {
        Rect::new(self.position.x, self.position.y, self.hitbox.x, self.hitbox.y)
    }

    /// Move a entidade tentando o eixo X e depois o Y, cancelando cada
    /// eixo separadamente se ele colidir com uma parede do mapa.
    pub fn move_and_collide(&mut self, map: &TileMap) {
        if !self.is_moving {
            self.animation = Animation::Idle;
            return;
        }
        self.animation = Animation::Running;

        let (dx, dy) = match self.direction {
            Direction::Up => (0.0, -self.speed),
            Direction::Down => (0.0, self.speed),
            Direction::Left => (-self.speed, 0.0),
            Direction::Right => (self.speed, 0.0),
        };

        let mut next = self.position;
        next.x += dx;
        next.x = next.x.clamp(0.0, WIDTH - self.hitbox.x);
        if !map.collides(Rect::new(next.x, self.position.y, self.hitbox.x, self.hitbox.y)) {
            self.position.x = next.x;
        }

        let mut next = self.position;
        next.y += dy;
        next.y = next.y.clamp(0.0, HEIGHT - self.hitbox.y);
        if !map.collides(Rect::new(self.position.x, next.y, self.hitbox.x, self.hitbox.y)) {
            self.position.y = next.y;
        }
    }

    pub fn tick_animation(&mut self) {
        self.count_frame += 1;
        if self.count_frame >= self.frame_delay {
            self.frame += 1;
            if self.frame >= 4 {
                self.frame = 0;
            }
            self.count_frame = 0;
        }
    }

    pub fn draw(&self, sprite: &Texture2D) {
        let fx = self.frame as f32 * self.hitbox.x;
        let fy = self.direction as i32 as f32 * self.hitbox.y;
        let source = if self.animation != Animation::Idle {
            Rect::new(fx, fy, self.hitbox.x, self.hitbox.y)
        } else {
            Rect::new(0.0, fy, self.hitbox.x, self.hitbox.y)
        };
        draw_texture_ex(
            sprite,
            self.position.x,
            self.position.y,
            WHITE,
            DrawTextureParams {
                source: Some(source),
                ..Default::default()
            },
        );
    }
}

/// Detecta encontro por sobreposição real de hitbox (AABB), em vez da
/// igualdade exata de coordenadas do `check_entity_colision` original.
pub fn overlaps(a: &Entity, b: &Entity) -> bool {
    a.rect().overlaps(&b.rect())
}

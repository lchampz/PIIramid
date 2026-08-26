//! Parser e colisão do mapa em tiles.
//! Substitui `init_map`/`draw_map` de `game.c`, que lia o arquivo caractere
//! a caractere com `fgetc`/`strcat`/`strtol`. Aqui o formato é texto legível:
//!
//! ```text
//! <colunas> <linhas>
//! <linha 0, um caractere por coluna: 0=chão, 1=parede>
//! <linha 1>
//! ...
//! ```

use macroquad::prelude::*;
use std::fmt;

use crate::config::TILE_SIZE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    Floor,
    Wall,
}

impl Tile {
    fn has_collision(self) -> bool {
        matches!(self, Tile::Wall)
    }
}

#[derive(Debug)]
pub struct TileMap {
    pub cols: usize,
    pub rows: usize,
    tiles: Vec<Vec<Tile>>,
}

#[derive(Debug)]
pub enum MapError {
    Io(String),
    EmptyFile,
    BadHeader(String),
    RowCountMismatch { expected: usize, found: usize },
    RowLengthMismatch { row: usize, expected: usize, found: usize },
    UnknownTile { row: usize, col: usize, ch: char },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::Io(e) => write!(f, "nao foi possivel abrir o mapa: {e}"),
            MapError::EmptyFile => write!(f, "arquivo de mapa vazio"),
            MapError::BadHeader(line) => write!(f, "cabecalho invalido: '{line}' (esperado 'colunas linhas')"),
            MapError::RowCountMismatch { expected, found } => {
                write!(f, "numero de linhas errado: esperado {expected}, encontrado {found}")
            }
            MapError::RowLengthMismatch { row, expected, found } => {
                write!(f, "linha {row} tem {found} colunas, esperado {expected}")
            }
            MapError::UnknownTile { row, col, ch } => {
                write!(f, "tile desconhecido '{ch}' em ({row}, {col})")
            }
        }
    }
}

impl TileMap {
    pub fn load(path: &str) -> Result<Self, MapError> {
        let text = std::fs::read_to_string(path).map_err(|e| MapError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, MapError> {
        let mut lines = text.lines();
        let header = lines.next().ok_or(MapError::EmptyFile)?;

        let mut parts = header.split_whitespace();
        let cols: usize = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MapError::BadHeader(header.to_string()))?;
        let rows: usize = parts
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| MapError::BadHeader(header.to_string()))?;

        let mut tiles = Vec::with_capacity(rows);
        for (row, line) in lines.enumerate() {
            if row >= rows {
                break;
            }
            if line.chars().count() != cols {
                return Err(MapError::RowLengthMismatch {
                    row,
                    expected: cols,
                    found: line.chars().count(),
                });
            }
            let mut parsed_row = Vec::with_capacity(cols);
            for (col, ch) in line.chars().enumerate() {
                let tile = match ch {
                    '0' => Tile::Floor,
                    '1' => Tile::Wall,
                    other => return Err(MapError::UnknownTile { row, col, ch: other }),
                };
                parsed_row.push(tile);
            }
            tiles.push(parsed_row);
        }

        if tiles.len() != rows {
            return Err(MapError::RowCountMismatch { expected: rows, found: tiles.len() });
        }

        Ok(TileMap { cols, rows, tiles })
    }

    fn tile_at(&self, row: usize, col: usize) -> Tile {
        self.tiles[row][col]
    }

    /// Verdadeiro se o retângulo (em pixels) sobrepõe algum tile com colisão.
    pub fn collides(&self, rect: Rect) -> bool {
        let left = (rect.x / TILE_SIZE).floor().max(0.0) as usize;
        let top = (rect.y / TILE_SIZE).floor().max(0.0) as usize;
        let right = ((rect.x + rect.w) / TILE_SIZE).ceil() as usize;
        let bottom = ((rect.y + rect.h) / TILE_SIZE).ceil() as usize;

        for row in top..bottom.min(self.rows) {
            for col in left..right.min(self.cols) {
                if self.tile_at(row, col).has_collision() {
                    let tile_rect = Rect::new(
                        col as f32 * TILE_SIZE,
                        row as f32 * TILE_SIZE,
                        TILE_SIZE,
                        TILE_SIZE,
                    );
                    if rect.overlaps(&tile_rect) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn draw(&self, floor: &Texture2D, wall: &Texture2D) {
        for row in 0..self.rows {
            for col in 0..self.cols {
                let tex = match self.tile_at(row, col) {
                    Tile::Floor => floor,
                    Tile::Wall => wall,
                };
                draw_texture(tex, col as f32 * TILE_SIZE, row as f32 * TILE_SIZE, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_map() {
        let map = TileMap::parse("3 2\n101\n000\n").unwrap();
        assert_eq!(map.cols, 3);
        assert_eq!(map.rows, 2);
        assert!(map.tile_at(0, 0).has_collision());
        assert!(!map.tile_at(0, 1).has_collision());
    }

    #[test]
    fn rejects_row_length_mismatch() {
        let err = TileMap::parse("3 2\n10\n000\n").unwrap_err();
        assert!(matches!(err, MapError::RowLengthMismatch { .. }));
    }

    #[test]
    fn rejects_unknown_tile() {
        let err = TileMap::parse("2 1\n1x\n").unwrap_err();
        assert!(matches!(err, MapError::UnknownTile { .. }));
    }

    #[test]
    fn collision_detects_wall_overlap() {
        let map = TileMap::parse("3 2\n101\n000\n").unwrap();
        assert!(map.collides(Rect::new(0.0, 0.0, 10.0, 10.0)));
        assert!(!map.collides(Rect::new(32.0, 0.0, 10.0, 10.0)));
    }
}

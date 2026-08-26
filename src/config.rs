//! Constantes globais de janela e tempo. A paleta de cores mora em
//! `ui::theme` (porta de `PIIramid Layout.dc.html`).

pub const WIDTH: f32 = 1280.0;
pub const HEIGHT: f32 = 720.0;

pub const NAME: &str = "PIIramid";

pub const TILE_SIZE: f32 = 32.0;

/// Todos os personagens (jogador e monstros) usam folhas de sprite no
/// mesmo grid: 4 direções (linhas) x 4 quadros de caminhada (colunas),
/// cada célula com este tamanho. Gerado por `tools/gen_assets.py`.
pub const SPRITE_FRAME: f32 = 64.0;

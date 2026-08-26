//! Paleta e tipografia do jogo — porta 1:1 da paleta definida em
//! `PIIramid Layout.dc.html` (tela "Guia de Estilo"): doze cores nomeadas,
//! usadas de forma consistente nas cinco telas.

use macroquad::prelude::Color;

const fn hex(rgb: u32) -> Color {
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    Color::new(r, g, b, 1.0)
}

pub const TUMBA: Color = hex(0x140e09);
pub const PEDRA: Color = hex(0x241a12);
pub const TIJOLO: Color = hex(0x3a2c20);
pub const AREIA_ESCURA: Color = hex(0x5a462f);
pub const POEIRA: Color = hex(0x8a7a62);
pub const PAPIRO: Color = hex(0xe8dcc0);
pub const OURO: Color = hex(0xe0a828);
pub const CHAMA: Color = hex(0xe07a3c);
pub const SANGUE: Color = hex(0xd9534f);
pub const VIDA: Color = hex(0x4ade5c);
pub const ESCARAVELHO: Color = hex(0x7fd4c1);
pub const MUSGO: Color = hex(0xb8d96a);

pub const DANGER_BG: Color = hex(0x2c1512);
pub const OK_BG: Color = hex(0x1a2016);

/// entradas da paleta para a tela de guia de estilo
pub const PALETTE: &[(&str, &str, Color)] = &[
    ("TUMBA", "#140e09", TUMBA),
    ("PEDRA", "#241a12", PEDRA),
    ("TIJOLO", "#3a2c20", TIJOLO),
    ("AREIA ESC.", "#5a462f", AREIA_ESCURA),
    ("POEIRA", "#8a7a62", POEIRA),
    ("PAPIRO", "#e8dcc0", PAPIRO),
    ("OURO", "#e0a828", OURO),
    ("CHAMA", "#e07a3c", CHAMA),
    ("SANGUE", "#d9534f", SANGUE),
    ("VIDA", "#4ade5c", VIDA),
    ("ESCARAVELHO", "#7fd4c1", ESCARAVELHO),
    ("MUSGO", "#b8d96a", MUSGO),
];

// tamanhos de fonte (papel: título grande/médio/pequeno, corpo grande/médio/pequeno)
pub const TITLE_XL: u16 = 64;
pub const TITLE_LG: u16 = 32;
pub const TITLE_MD: u16 = 22;
pub const TITLE_SM: u16 = 15;
pub const BODY_LG: u16 = 19;
pub const BODY_MD: u16 = 16;
pub const BODY_SM: u16 = 14;

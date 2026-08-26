//! Grimório — porta de `PIIramid Layout.dc.html` (tela "Equipamento").
//!
//! **Mockada por decisão explícita** (ver `Roadmap.md` / RFC-002 em
//! `C:\docs\Piiramid`): não existe sistema de inventário real ainda —
//! slots, mochila e scripts salvos aqui são dados fixos, só pra a tela
//! existir e ser navegável. Equipar/carregar não muda nada no jogo.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::WIDTH;
use crate::scenes::Transition;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

struct Slot {
    name: &'static str,
    item: &'static str,
    stat: &'static str,
}

struct BagItem {
    name: &'static str,
    desc: &'static str,
    qty: &'static str,
}

struct SavedScript {
    name: &'static str,
    cost: &'static str,
    body: &'static str,
}

const SLOTS: &[Slot] = &[
    Slot { name: "ARMA", item: "Khopesh Trincado", stat: "+14 fisico - 4 ciclos" },
    Slot { name: "MAGIA", item: "Chama do Oasis", stat: "+22 fogo - 6 ciclos" },
    Slot { name: "ESCUDO", item: "Bronze Lascado", stat: "-8 dano recebido" },
    Slot { name: "POCAO", item: "Seiva de Lotus", stat: "+24 vida - x3" },
    Slot { name: "AMULETO", item: "Escaravelho Azul", stat: "+2 ciclos por turno" },
    Slot { name: "RELIQUIA", item: "Vazio", stat: "Encontre na camara IV" },
];

const BAG: &[BagItem] = &[
    BagItem { name: "Seiva de Lotus", desc: "Restaura 24 de vida.", qty: "x3" },
    BagItem { name: "Oleo de Mariposa", desc: "Proxima magia.Fogo causa x3.", qty: "x1" },
    BagItem { name: "Areia Calcinada", desc: "Componente de itens futuros.", qty: "x8" },
    BagItem { name: "Papiro em Branco", desc: "Salva um script no grimorio.", qty: "x2" },
    BagItem { name: "Chave de Obsidiana", desc: "Abre a antecamara selada.", qty: "x1" },
];

const SCRIPTS: &[SavedScript] = &[
    SavedScript { name: "abre-fogo.pii", cost: "10 ciclos", body: "atacar(magia.Fogo)\ndefender(escudo.Bronze)" },
    SavedScript {
        name: "defensivo.pii",
        cost: "12 ciclos",
        body: "if eu.vida < 40:\n    curar(pocao.Vida)\ndefender(escudo.Bronze)",
    },
    SavedScript { name: "sonda.pii", cost: "1 ciclo", body: "esperar()" },
];

pub struct GrimoireScene {
    btn_back: Button,
}

impl GrimoireScene {
    pub fn new() -> Self {
        GrimoireScene { btn_back: Button::new("VOLTAR", vec2(WIDTH - 160.0, 14.0), vec2(140.0, 40.0), ButtonStyle::Ghost, 13) }
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = mouse_position().into();
        self.btn_back.update_hover(mouse);
        if self.btn_back.clicked(mouse) || is_key_pressed(KeyCode::Escape) {
            return Some(Transition::GoToMenu);
        }
        None
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);

        draw_rectangle(0.0, 0.0, WIDTH, 62.0, theme::PEDRA);
        draw_rectangle(0.0, 59.0, WIDTH, 3.0, theme::OURO);
        draw_text_ex("GRIMORIO", 20.0, 30.0, TextParams { font: Some(&assets.font_title), font_size: 15, color: theme::OURO, ..Default::default() });
        draw_text_ex(
            "EQUIPAMENTO - SCRIPTS SALVOS (dados de exemplo)",
            20.0,
            50.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() },
        );
        draw_text_ex(
            "248 ESCARAVELHOS",
            WIDTH - 480.0,
            36.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_LG, color: theme::MUSGO, ..Default::default() },
        );
        self.btn_back.draw(&assets.font_body);

        self.draw_slots_column(assets);
        self.draw_bag_column(assets);
        self.draw_scripts_column(assets);
    }

    fn draw_slots_column(&self, assets: &Assets) {
        let x = 20.0;
        let w = 420.0;
        let mut y = 90.0;
        draw_text_ex("SLOTS", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        y += 16.0;

        let col_w = (w - 12.0) / 2.0;
        for (i, slot) in SLOTS.iter().enumerate() {
            let cx = x + (i % 2) as f32 * (col_w + 12.0);
            let cy = y + (i / 2) as f32 * 92.0;
            draw_rectangle(cx, cy, col_w, 82.0, theme::PEDRA);
            draw_rectangle_lines(cx, cy, col_w, 82.0, 2.0, theme::TIJOLO);
            draw_text_ex(slot.name, cx + 10.0, cy + 20.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::AREIA_ESCURA, ..Default::default() });
            draw_text_ex(slot.item, cx + 10.0, cy + 44.0, TextParams { font: Some(&assets.font_body), font_size: 15, color: theme::PAPIRO, ..Default::default() });
            draw_text_ex(slot.stat, cx + 10.0, cy + 66.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() });
        }

        let attrs_y = y + 3.0 * 92.0 + 20.0;
        draw_text_ex("ATRIBUTOS", x, attrs_y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        let stats: [(&str, &str, f32); 5] =
            [("VIDA MAX", "100", 0.62), ("CICLOS/TURNO", "20", 0.55), ("FORCA", "14", 0.48), ("ARCANO", "22", 0.76), ("SORTE", "07", 0.24)];
        let mut sy = attrs_y + 16.0;
        for (label, value, pct) in stats {
            draw_text_ex(label, x, sy + 12.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::POEIRA, ..Default::default() });
            let bar_x = x + 150.0;
            let bar_w = w - 150.0 - 40.0;
            draw_rectangle(bar_x, sy, bar_w, 14.0, theme::PEDRA);
            draw_rectangle(bar_x, sy, bar_w * pct, 14.0, theme::OURO);
            draw_text_ex(value, bar_x + bar_w + 8.0, sy + 12.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::PAPIRO, ..Default::default() });
            sy += 26.0;
        }
    }

    fn draw_bag_column(&self, assets: &Assets) {
        let x = 460.0;
        let w = 400.0;
        let mut y = 90.0;
        draw_text_ex("MOCHILA", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        y += 20.0;
        for item in BAG {
            draw_rectangle(x, y, w, 58.0, theme::PEDRA);
            draw_rectangle_lines(x, y, w, 58.0, 2.0, theme::TIJOLO);
            draw_text_ex(item.name, x + 12.0, y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::ESCARAVELHO, ..Default::default() });
            draw_text_ex(item.desc, x + 12.0, y + 42.0, TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() });
            let qty_dims = measure_text(item.qty, Some(&assets.font_body), 14, 1.0);
            draw_text_ex(item.qty, x + w - qty_dims.width - 12.0, y + 33.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::AREIA_ESCURA, ..Default::default() });
            y += 64.0;
        }
    }

    fn draw_scripts_column(&self, assets: &Assets) {
        let x = 880.0;
        let w = WIDTH - x - 20.0;
        let mut y = 90.0;
        draw_text_ex("SCRIPTS SALVOS", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        y += 20.0;
        for script in SCRIPTS {
            let body_lines = script.body.lines().count() as f32;
            let h = 46.0 + body_lines * 18.0 + 40.0;
            draw_rectangle(x, y, w, h, theme::TUMBA);
            draw_rectangle_lines(x, y, w, h, 2.0, theme::AREIA_ESCURA);
            draw_text_ex(script.name, x + 12.0, y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::PAPIRO, ..Default::default() });
            let cost_dims = measure_text(script.cost, Some(&assets.font_body), 12, 1.0);
            draw_text_ex(script.cost, x + w - cost_dims.width - 12.0, y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::AREIA_ESCURA, ..Default::default() });
            let mut by = y + 42.0;
            for line in script.body.lines() {
                draw_text_ex(line, x + 12.0, by, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::ESCARAVELHO, ..Default::default() });
                by += 18.0;
            }
            y += h + 14.0;
        }
    }
}

//! Grimório — porta de `PIIramid Layout.dc.html` (tela "Equipamento").
//!
//! RFC-002: deixa de ser mockado. `SLOTS`/`BAG`/`SCRIPTS` viravam dados
//! fixos (`&'static`); agora a tela carrega `SaveData` real
//! (`SaveData::load()`) ao entrar, deixa o jogador equipar/desequipar
//! clicando, e grava de volta no disco ao apertar VOLTAR. Amuleto/Relíquia
//! continuam mockados (não-objetivo 3 da RFC): `ItemKind` só tem 4
//! variantes reais (Espada/Magia/Escudo/Poção) — os outros dois slots não
//! têm categoria correspondente na linguagem ainda.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::WIDTH;
use crate::inventory::{Item, PlayerClass, SaveData};
use crate::scenes::Transition;
use crate::screen_scale::virtual_mouse_position;
use crate::script::value::ItemKind;
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::theme;

/// Os 4 slots reais, na ordem em que aparecem na grade (RFC-002). Usado
/// só para desenhar/testar clique na posição certa — a fonte de verdade
/// do que está equipado é sempre `SaveData::loadout`.
const REAL_SLOT_KINDS: [ItemKind; 4] = [ItemKind::Espada, ItemKind::Magia, ItemKind::Escudo, ItemKind::Pocao];

fn slot_display_name(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Espada => "ARMA",
        ItemKind::Magia => "MAGIA",
        ItemKind::Escudo => "ESCUDO",
        ItemKind::Pocao => "POCAO",
    }
}

pub struct GrimoireScene {
    btn_back: Button,
    save: SaveData,
}

impl GrimoireScene {
    pub fn new() -> Self {
        GrimoireScene {
            btn_back: Button::new("VOLTAR", vec2(WIDTH - 160.0, 14.0), vec2(140.0, 40.0), ButtonStyle::Ghost, 13),
            save: SaveData::load(),
        }
    }

    pub fn update(&mut self) -> Option<Transition> {
        let mouse: Vec2 = virtual_mouse_position().into();
        self.btn_back.update_hover(mouse);

        if is_mouse_button_pressed(MouseButton::Left) {
            self.handle_click(mouse);
        }

        if self.btn_back.clicked(mouse) || is_key_pressed(KeyCode::Escape) {
            // RFC-002, regra 3/8: grava o inventário/scripts no disco só
            // ao sair da tela -- é o que faz "fechar e abrir o jogo manter
            // inventário" (critério de aceite) valer também pra quem só
            // visitou o Grimório sem entrar em duelo.
            self.save.save();
            return Some(Transition::GoToMenu { last_drop: None });
        }
        None
    }

    /// Clicar num item da mochila equipa/desequipa (RFC-002, regra 9):
    /// clicar num item já equipado desequipa (devolve à mochila); clicar
    /// num item da mochila equipa no slot do `kind` correspondente,
    /// devolvendo o que já estava equipado (se houver) de volta à mochila.
    fn handle_click(&mut self, mouse: Vec2) {
        for (i, kind) in REAL_SLOT_KINDS.iter().enumerate() {
            if slot_rect(i).contains(mouse) && self.save.loadout.slot(*kind).is_some() {
                self.unequip(*kind);
                return;
            }
        }
        for i in 0..self.save.bag.0.len() {
            if bag_row_rect(i).contains(mouse) {
                self.equip_from_bag(i);
                return;
            }
        }
        // RFC-003 §1, regra 6: clicar numa classe seleciona e salva
        // imediatamente -- mesmo padrão de persistência imediata que
        // equipar/desequipar já usa (nenhum botão "confirmar" separado).
        for (i, class) in PlayerClass::ALL.iter().enumerate() {
            if class_button_rect(i).contains(mouse) {
                self.save.player_class = Some(*class);
                self.save.save();
                return;
            }
        }
    }

    fn equip_from_bag(&mut self, bag_index: usize) {
        let (item, qty) = self.save.bag.0[bag_index].clone();
        let kind = item.kind;

        if let Some(previous) = self.save.loadout.slot_mut(kind).replace(item) {
            self.return_to_bag(previous);
        }

        if qty > 1 {
            self.save.bag.0[bag_index].1 -= 1;
        } else {
            self.save.bag.0.remove(bag_index);
        }
    }

    fn unequip(&mut self, kind: ItemKind) {
        if let Some(item) = self.save.loadout.slot_mut(kind).take() {
            self.return_to_bag(item);
        }
    }

    fn return_to_bag(&mut self, item: Item) {
        if let Some(entry) = self.save.bag.0.iter_mut().find(|(it, _)| it.id == item.id) {
            entry.1 += 1;
        } else {
            self.save.bag.0.push((item, 1));
        }
    }

    pub fn draw(&self, assets: &Assets) {
        clear_background(theme::TUMBA);

        draw_rectangle(0.0, 0.0, WIDTH, 62.0, theme::PEDRA);
        draw_rectangle(0.0, 59.0, WIDTH, 3.0, theme::OURO);
        draw_text_ex("GRIMORIO", 20.0, 30.0, TextParams { font: Some(&assets.font_title), font_size: 15, color: theme::OURO, ..Default::default() });
        draw_text_ex(
            "EQUIPAMENTO - CLIQUE NA MOCHILA PRA EQUIPAR, NO SLOT PRA DESEQUIPAR",
            20.0,
            50.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() },
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

        for (i, kind) in REAL_SLOT_KINDS.iter().enumerate() {
            let r = slot_rect(i);
            let equipped = self.save.loadout.slot(*kind);
            draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, theme::TIJOLO);
            draw_text_ex(
                slot_display_name(*kind),
                r.x + 10.0,
                r.y + 20.0,
                TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::AREIA_ESCURA, ..Default::default() },
            );
            let (label, stat) = match equipped {
                Some(item) => (item.name.as_str(), format!("+{} dano - {}", item.bonus_damage, item.id)),
                None => ("Vazio", "Clique num item da mochila".to_string()),
            };
            draw_text_ex(label, r.x + 10.0, r.y + 44.0, TextParams { font: Some(&assets.font_body), font_size: 15, color: theme::PAPIRO, ..Default::default() });
            draw_text_ex(&stat, r.x + 10.0, r.y + 66.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() });
        }

        // Amuleto/Relíquia (não-objetivo 3 da RFC-002): permanecem
        // mockados de propósito -- `ItemKind` não tem categoria pra eles.
        let mock_y = y + 2.0 * 92.0;
        for (i, (name, item, stat)) in [
            ("AMULETO", "Escaravelho Azul", "+2 ciclos por turno"),
            ("RELIQUIA", "Vazio", "Encontre na camara IV"),
        ]
        .into_iter()
        .enumerate()
        {
            let col_w = (w - 12.0) / 2.0;
            let cx = x + (i % 2) as f32 * (col_w + 12.0);
            let cy = mock_y + (i / 2) as f32 * 92.0;
            draw_rectangle(cx, cy, col_w, 82.0, theme::PEDRA);
            draw_rectangle_lines(cx, cy, col_w, 82.0, 2.0, theme::TIJOLO);
            draw_text_ex(name, cx + 10.0, cy + 20.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::AREIA_ESCURA, ..Default::default() });
            draw_text_ex(item, cx + 10.0, cy + 44.0, TextParams { font: Some(&assets.font_body), font_size: 15, color: theme::PAPIRO, ..Default::default() });
            draw_text_ex(stat, cx + 10.0, cy + 66.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() });
        }

        // RFC-003 §1, regra 6: seletor de classe -- 3 opções clicáveis,
        // clicar seleciona e salva imediatamente (`handle_click`). A
        // classe atual (`self.save.player_class`) fica destacada em OURO.
        let class_y = mock_y + 92.0 + 20.0;
        draw_text_ex("CLASSE", x, class_y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        for (i, class) in PlayerClass::ALL.iter().enumerate() {
            let r = class_button_rect(i);
            let selected = self.save.player_class == Some(*class);
            let (bg, border, label_color) = if selected { (theme::OURO, theme::OURO, theme::TUMBA) } else { (theme::PEDRA, theme::TIJOLO, theme::PAPIRO) };
            draw_rectangle(r.x, r.y, r.w, r.h, bg);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, border);
            let dims = measure_text(class.label(), Some(&assets.font_body), 13, 1.0);
            draw_text_ex(
                class.label(),
                r.x + (r.w - dims.width) / 2.0,
                r.y + r.h / 2.0 + 5.0,
                TextParams { font: Some(&assets.font_body), font_size: 13, color: label_color, ..Default::default() },
            );
        }

        let attrs_y = class_y + 16.0 + 40.0 + 18.0;
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
        let mut y = 90.0;
        draw_text_ex("MOCHILA (clique pra equipar)", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        y += 20.0;
        if self.save.bag.0.is_empty() {
            draw_text_ex(
                "Vazia por enquanto -- vencer um duelo derruba o despojo do monstro aqui (RFC-028).",
                x,
                y + 20.0,
                TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::AREIA_ESCURA, ..Default::default() },
            );
        }
        for (i, (item, qty)) in self.save.bag.0.iter().enumerate() {
            let r = bag_row_rect(i);
            draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, theme::TIJOLO);
            draw_text_ex(&item.name, r.x + 12.0, r.y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::ESCARAVELHO, ..Default::default() });
            let desc = format!("{} - bonus +{}", slot_display_name(item.kind), item.bonus_damage);
            draw_text_ex(&desc, r.x + 12.0, r.y + 42.0, TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() });
            let qty_label = format!("x{qty}");
            let qty_dims = measure_text(&qty_label, Some(&assets.font_body), 14, 1.0);
            draw_text_ex(&qty_label, r.x + r.w - qty_dims.width - 12.0, r.y + 33.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::AREIA_ESCURA, ..Default::default() });
        }
    }

    fn draw_scripts_column(&self, assets: &Assets) {
        let x = 880.0;
        let w = WIDTH - x - 20.0;
        let mut y = 90.0;
        draw_text_ex("SCRIPTS SALVOS", x, y, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() });
        y += 20.0;
        if self.save.scripts.is_empty() {
            draw_text_ex(
                "Nenhum ainda -- use SALVAR na tela de duelo.",
                x,
                y + 20.0,
                TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::AREIA_ESCURA, ..Default::default() },
            );
        }
        for script in &self.save.scripts {
            let body_lines = script.body.lines().count() as f32;
            let h = 46.0 + body_lines * 18.0 + 12.0;
            draw_rectangle(x, y, w, h, theme::TUMBA);
            draw_rectangle_lines(x, y, w, h, 2.0, theme::AREIA_ESCURA);
            draw_text_ex(&script.name, x + 12.0, y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::PAPIRO, ..Default::default() });
            let mut by = y + 42.0;
            for line in script.body.lines() {
                draw_text_ex(line, x + 12.0, by, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::ESCARAVELHO, ..Default::default() });
                by += 18.0;
            }
            y += h + 14.0;
        }
    }
}

/// Retângulo do slot `i` (0..4) na grade 2 colunas — mesmo layout de
/// `draw_slots_column`, extraído aqui pra que clique e desenho nunca
/// divirjam.
fn slot_rect(i: usize) -> Rect {
    let x = 20.0;
    let w = 420.0;
    let y = 90.0 + 16.0;
    let col_w = (w - 12.0) / 2.0;
    let cx = x + (i % 2) as f32 * (col_w + 12.0);
    let cy = y + (i / 2) as f32 * 92.0;
    Rect::new(cx, cy, col_w, 82.0)
}

/// Retângulo da linha `i` da mochila — mesmo layout de `draw_bag_column`.
fn bag_row_rect(i: usize) -> Rect {
    let x = 460.0;
    let w = 400.0;
    let y = 90.0 + 20.0;
    Rect::new(x, y + i as f32 * 64.0, w, 58.0)
}

/// Retângulo do botão de classe `i` (0..3, mesma ordem de `PlayerClass::ALL`)
/// — extraído aqui pra que clique (`handle_click`) e desenho
/// (`draw_slots_column`) nunca divirjam, mesmo padrão de `slot_rect`.
fn class_button_rect(i: usize) -> Rect {
    let x = 20.0;
    let w = 420.0;
    let y = 90.0 + 16.0 + 2.0 * 92.0 + 92.0 + 20.0 + 16.0;
    let gap = 8.0;
    let btn_w = (w - 2.0 * gap) / 3.0;
    Rect::new(x + i as f32 * (btn_w + gap), y, btn_w, 40.0)
}

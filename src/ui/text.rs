//! Quebra de texto por largura real, medida em pixels (RFC-034).
//!
//! Antes desta RFC, `wrap_text` existia duplicada em 4 arquivos
//! (`scenes/{intro,gameover,duel,menu}.rs`), toda vez decidindo onde
//! quebrar linha por **contagem de caracteres** com um `max_chars`
//! calibrado a olho. O `.ttf` de Silkscreen não é monoespaçado -- "i" e
//! "M" não ocupam a mesma largura -- então um limite de caracteres que
//! "parecia certo" num teste visual estourava a borda do card em outro
//! texto (caso confirmado: `intro.rs`, painel 5, "CADA" ultrapassando o
//! card em 57px -- ver RFC-034). Esta função substitui a heurística por
//! `measure_text` de verdade: só quebra quando a linha candidata
//! realmente não cabe.
//!
//! Continua quebrando só em espaço -- nunca no meio de uma palavra, nem
//! hifenizando (não-objetivo da RFC). Uma palavra sozinha mais larga que
//! `max_width` (não deveria acontecer com os textos do jogo, mas não é
//! impossível) ainda vira sua própria linha e pode ultrapassar a borda --
//! preferimos vazar um pouco a cortar caracteres, mesmo trade-off que
//! `duel.rs::wrap_text_px` (RFC-033) já tinha antes desta função existir.

use macroquad::prelude::{measure_text, Font};

/// Quebra `text` em linhas que cabem em `max_width`, medidas com `font`
/// no tamanho `font_size` -- o mesmo par fonte/tamanho já usado no
/// `draw_text_ex` correspondente, nunca um valor novo.
pub fn wrap_text(text: &str, font: &Font, font_size: u16, max_width: f32) -> Vec<String> {
    wrap_by_width(text, max_width, |s| measure_text(s, Some(font), font_size, 1.0).width)
}

/// Núcleo do algoritmo greedy, desacoplado do tipo `Font` do macroquad --
/// só assim dá pra testar contra métricas reais de fonte sem precisar de
/// contexto gráfico (`measure_text` do macroquad exige uma janela/GPU
/// viva; `#[test]` comum não tem uma, ver `mod tests` abaixo).
fn wrap_by_width(text: &str, max_width: f32, mut width_of: impl FnMut(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if !current.is_empty() && width_of(&candidate) > max_width {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::gameover::{FLAVOR_LOST, FLAVOR_WON, FLAVOR_MAX_TEXT_WIDTH};
    use crate::scenes::intro::{INTRO_PANELS, PANEL_TEXT_MAX_WIDTH};
    use crate::scenes::menu::{DESC_TEXT, DESC_TEXT_MAX_WIDTH};
    use crate::monsters::PHASES;
    use crate::scenes::duel::DOSSIER_TEXT_MAX_WIDTH;
    use crate::ui::theme::BODY_LG;
    use std::collections::HashMap;

    /// Leitor mínimo de métricas de um `.ttf` (tabelas `head`/`hhea`/`maxp`/
    /// `hmtx`/`cmap` formato 4), reimplementado à mão em `std` puro --
    /// **de propósito não usa `fontdue`** (a lib que o macroquad usa por
    /// baixo pra rasterizar): `fontdue` é dependência transitiva, não
    /// direta, então importá-la aqui exigiria adicionar uma dependência
    /// nova ao `Cargo.toml` só pra teste, o que a RFC-034 não pediu e o
    /// contrato do gamedev não permite sem uma RFC própria. Como
    /// `measure_text` do macroquad soma só a largura de avanço (`advance`)
    /// de cada glifo, sem kerning (ver `macroquad::text::Font::measure_text`),
    /// somar `hmtx.advanceWidth` escalado por `font_size/unitsPerEm`
    /// reproduz exatamente esse número -- é a mesma matemática que a
    /// investigação desta RFC já fez uma vez à mão via `fontTools` no
    /// `.ttf` de Silkscreen-Regular.
    struct TtfMetrics {
        units_per_em: u16,
        advances: Vec<u16>,
        cmap: HashMap<u32, u16>,
    }

    fn u16_at(b: &[u8], off: usize) -> u16 {
        u16::from_be_bytes([b[off], b[off + 1]])
    }
    fn i16_at(b: &[u8], off: usize) -> i16 {
        i16::from_be_bytes([b[off], b[off + 1]])
    }
    fn u32_at(b: &[u8], off: usize) -> u32 {
        u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    }

    fn find_table(data: &[u8], tag: &[u8; 4]) -> (usize, usize) {
        let num_tables = u16_at(data, 4) as usize;
        for i in 0..num_tables {
            let rec = 12 + i * 16;
            if &data[rec..rec + 4] == tag {
                return (u32_at(data, rec + 8) as usize, u32_at(data, rec + 12) as usize);
            }
        }
        panic!("tabela '{}' nao encontrada no .ttf", String::from_utf8_lossy(tag));
    }

    impl TtfMetrics {
        fn load(data: &[u8]) -> Self {
            let (head_off, _) = find_table(data, b"head");
            let units_per_em = u16_at(data, head_off + 18);

            let (hhea_off, _) = find_table(data, b"hhea");
            let num_h_metrics = u16_at(data, hhea_off + 34) as usize;

            let (maxp_off, _) = find_table(data, b"maxp");
            let num_glyphs = u16_at(data, maxp_off + 4) as usize;

            let (hmtx_off, _) = find_table(data, b"hmtx");
            let mut advances = Vec::with_capacity(num_glyphs);
            for i in 0..num_h_metrics {
                advances.push(u16_at(data, hmtx_off + i * 4));
            }
            let last = *advances.last().unwrap_or(&0);
            while advances.len() < num_glyphs {
                advances.push(last);
            }

            let (cmap_off, _) = find_table(data, b"cmap");
            let num_subtables = u16_at(data, cmap_off + 2) as usize;
            let mut subtable_off = None;
            for i in 0..num_subtables {
                let rec = cmap_off + 4 + i * 8;
                let platform_id = u16_at(data, rec);
                let encoding_id = u16_at(data, rec + 2);
                let offset = u32_at(data, rec + 4) as usize;
                if platform_id == 0 || (platform_id == 3 && encoding_id == 1) {
                    subtable_off = Some(cmap_off + offset);
                }
            }
            let subtable_off = subtable_off.expect("cmap unicode (formato 4) nao encontrada");
            assert_eq!(u16_at(data, subtable_off), 4, "esperava cmap formato 4 (BMP) -- suficiente pro texto ascii/latino do jogo");

            let seg_count = u16_at(data, subtable_off + 6) as usize / 2;
            let end_codes_off = subtable_off + 14;
            let start_codes_off = end_codes_off + seg_count * 2 + 2; // +2 pula reservedPad
            let id_delta_off = start_codes_off + seg_count * 2;
            let id_range_offset_off = id_delta_off + seg_count * 2;

            let mut cmap = HashMap::new();
            for seg in 0..seg_count {
                let end_code = u16_at(data, end_codes_off + seg * 2) as u32;
                let start_code = u16_at(data, start_codes_off + seg * 2) as u32;
                let id_delta = i16_at(data, id_delta_off + seg * 2);
                let id_range_offset = u16_at(data, id_range_offset_off + seg * 2);
                if start_code == 0xFFFF {
                    continue;
                }
                for code in start_code..=end_code {
                    if code == 0xFFFF {
                        continue;
                    }
                    let glyph_id = if id_range_offset == 0 {
                        ((code as i32 + id_delta as i32) & 0xFFFF) as u16
                    } else {
                        let addr = id_range_offset_off + seg * 2 + id_range_offset as usize + (code - start_code) as usize * 2;
                        let raw = u16_at(data, addr);
                        if raw == 0 { 0 } else { ((raw as i32 + id_delta as i32) & 0xFFFF) as u16 }
                    };
                    if glyph_id != 0 {
                        cmap.insert(code, glyph_id);
                    }
                }
            }

            TtfMetrics { units_per_em, advances, cmap }
        }

        fn width(&self, text: &str, font_size: u16) -> f32 {
            let scale = font_size as f32 / self.units_per_em as f32;
            text.chars()
                .map(|c| {
                    let gid = *self.cmap.get(&(c as u32)).unwrap_or(&0) as usize;
                    self.advances.get(gid).copied().unwrap_or(0) as f32 * scale
                })
                .sum()
        }
    }

    fn silkscreen() -> TtfMetrics {
        TtfMetrics::load(include_bytes!("../../assets/fonts/Silkscreen-Regular.ttf"))
    }

    /// Prova numérica central da RFC-034: quebra cada texto real do jogo
    /// com `wrap_by_width` alimentada pela métrica real (independente do
    /// `fontdue`/macroquad -- ver doc de `TtfMetrics`) e re-mede cada
    /// linha resultante. Se alguma ultrapassar a largura útil do
    /// container correspondente, o teste falha -- mesmo espírito de
    /// `ui::theme::tests::informative_text_pairs_meet_wcag_contrast`.
    fn assert_all_lines_fit(label: &str, text: &str, font_size: u16, max_width: f32, metrics: &TtfMetrics) {
        for line in wrap_by_width(text, max_width, |s| metrics.width(s, font_size)) {
            let w = metrics.width(&line, font_size);
            assert!(
                w <= max_width,
                "{label}: linha \"{line}\" mede {w:.1}px, largura util e {max_width:.1}px (font_size {font_size})"
            );
        }
    }

    #[test]
    fn intro_panels_never_overflow_the_card() {
        let metrics = silkscreen();
        for (i, [line1, line2]) in INTRO_PANELS.iter().enumerate() {
            assert_all_lines_fit(&format!("intro painel {i} linha 1"), line1, BODY_LG, PANEL_TEXT_MAX_WIDTH, &metrics);
            assert_all_lines_fit(&format!("intro painel {i} linha 2"), line2, BODY_LG, PANEL_TEXT_MAX_WIDTH, &metrics);
        }
    }

    #[test]
    fn gameover_flavor_texts_never_overflow_the_card() {
        let metrics = silkscreen();
        assert_all_lines_fit("gameover flavor (vitoria)", FLAVOR_WON, BODY_LG, FLAVOR_MAX_TEXT_WIDTH, &metrics);
        assert_all_lines_fit("gameover flavor (derrota)", FLAVOR_LOST, BODY_LG, FLAVOR_MAX_TEXT_WIDTH, &metrics);
    }

    #[test]
    fn menu_description_never_overflows_the_left_column() {
        let metrics = silkscreen();
        assert_all_lines_fit("menu DESC_TEXT", DESC_TEXT, BODY_LG, DESC_TEXT_MAX_WIDTH, &metrics);
    }

    #[test]
    fn monster_dossier_descriptions_never_overflow_the_side_panel() {
        let metrics = silkscreen();
        for (kind, spec_fn) in PHASES {
            let spec = spec_fn();
            for (i, line) in spec.description.iter().enumerate() {
                assert_all_lines_fit(&format!("dossie {kind:?} descricao linha {i}"), line, 14, DOSSIER_TEXT_MAX_WIDTH, &metrics);
            }
        }
    }

    #[test]
    fn wrap_by_width_never_splits_a_word() {
        // regra de não-objetivo da RFC: nunca corta palavra ao meio, nem
        // hifeniza -- só quebra em espaço. Largura minúscula de propósito
        // pra forçar quebra em quase toda palavra.
        let words = ["Sob", "a", "piramide", "arde", "uma", "brasa"];
        let text = words.join(" ");
        let lines = wrap_by_width(&text, 1.0, |s| s.chars().count() as f32);
        let rebuilt: Vec<&str> = lines.iter().flat_map(|l| l.split_whitespace()).collect();
        assert_eq!(rebuilt, words);
    }
}

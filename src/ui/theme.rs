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

// Contrato de cor (RFC-007) — cada constante abaixo tem um papel único.
// Divergência entre este comentário e o uso real em `scenes/` é bug, não
// liberdade de estilo. `ui::theme::tests::informative_text_pairs_meet_wcag_contrast`
// (regra 10) trava os pares de texto informativo que aparecem aqui.

/// Fundo mais profundo. Base do `clear_background` da cena e trilha
/// (vazia) dos medidores de barra — ciclos, carga. Nunca como texto.
pub const TUMBA: Color = hex(0x140e09);

/// Fundo de painel — superfície-base de topo, editor, dossiê, log,
/// cartão de comando. Nunca como texto ou borda.
pub const PEDRA: Color = hex(0x241a12);

/// Borda e divisória neutra — moldura de painel/cartão secundário
/// sobre `PEDRA` ou mais claro. Nunca sobre `TUMBA` (contraste 1.42,
/// quase invisível) — ali a divisória usa `AREIA_ESCURA`. Nunca como
/// preenchimento nem como texto.
pub const TIJOLO: Color = hex(0x3a2c20);

/// Superfície elevada — preenchimento de um elemento que se destaca do
/// fundo (ex.: etiqueta de fraqueza do monstro). Também é a cor de
/// divisória quando o fundo por baixo é `TUMBA` — caso em que `TIJOLO`
/// desaparece — porque ali ela é decoração, não informação, e é a
/// única excecão declarada a "nunca como borda". Nunca como texto.
pub const AREIA_ESCURA: Color = hex(0x5a462f);

/// Texto secundário / desabilitado — rótulo de apoio, número de linha,
/// contagem de linhas, custo de comando, descrição do monstro. Nenhum
/// desses textos usa `AREIA_ESCURA` — o contraste dela reprova como
/// texto em qualquer fundo do jogo.
pub const POEIRA: Color = hex(0x8a7a62);

/// Texto principal — nome, diálogo, conteúdo de leitura primária.
pub const PAPIRO: Color = hex(0xe8dcc0);

/// Foco e valor — moldura de painel ativo (editor, retratos, dossiê,
/// barra de vida) e número/valor em destaque (golpe bônus). Não é a
/// cor da carga (isso é `CHAMA`) nem do rótulo de postura (isso é
/// `PAPIRO` — postura é dado primário, não destaque).
pub const OURO: Color = hex(0xe0a828);

/// Carga e alerta — exclusiva do preenchimento e, quando a carga está
/// cheia, da moldura pulsante da barra de intenção/carga. Nunca em
/// destaque de sintaxe do editor (isso é `ESCARAVELHO`).
pub const CHAMA: Color = hex(0xe07a3c);

/// Dano e erro — texto de erro do interpretador, evento de
/// contra-ataque/golpe especial no log, preenchimento da barra de vida
/// do inimigo. Nunca para identidade visual do inimigo em si (borda de
/// retrato, tag de fraqueza) — isso é `TIJOLO`/`AREIA_ESCURA`.
pub const SANGUE: Color = hex(0xd9534f);

/// Vida e sucesso — preenchimento da barra de vida do jogador, evento
/// de cura e de ataque efetivo no log.
pub const VIDA: Color = hex(0x4ade5c);

/// Acento frio / informação — barra de ciclos, rótulo e custo dos
/// comandos clicáveis, palavras-chave de controle do editor
/// (`if`/`while`/`for`/...), eventos informativos do log (defender,
/// inspecionar).
pub const ESCARAVELHO: Color = hex(0x7fd4c1);

/// Acento secundário — exclusiva do token de valor no editor (número,
/// string, valor de enum como `Fogo`/`Bronze`). Nunca em evento de
/// combate do log — resultado de ataque é `VIDA` (efetivo) ou `POEIRA`
/// (fraco).
pub const MUSGO: Color = hex(0xb8d96a);

/// Fundo do estado de erro — acompanha texto/borda `SANGUE`.
pub const DANGER_BG: Color = hex(0x2c1512);
/// Fundo do estado de sucesso — acompanha texto/borda de confirmação.
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

// Helpers de contraste (RFC-007, regra 10) — só existem para o teste
// abaixo hoje, mas ficam fora de `mod tests` de propósito: são a
// ferramenta que qualquer par de cor novo deveria passar por, não um
// detalhe de implementação do teste. `#[cfg(test)]` evita o aviso de
// código morto sem esconder a função dentro do módulo de teste.

/// luminância relativa sRGB (WCAG 2.x) de um canal já normalizado em
/// `0.0..=1.0` — é o mesmo cálculo que a auditoria de design
/// (`AUDITORIA-identidade-visual.md`) fez à mão duas vezes de forma
/// independente para verificar contraste.
#[cfg(test)]
fn channel_luminance(v: f32) -> f32 {
    if v <= 0.03928 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// luminância relativa de uma cor (`0.2126 R + 0.7152 G + 0.0722 B`,
/// com cada canal passado por `channel_luminance`).
#[cfg(test)]
fn relative_luminance(c: Color) -> f32 {
    0.2126 * channel_luminance(c.r) + 0.7152 * channel_luminance(c.g) + 0.0722 * channel_luminance(c.b)
}

/// razão de contraste WCAG entre duas cores — `(L_clara + 0.05) / (L_escura + 0.05)`.
/// Sempre >= 1.0; a ordem dos argumentos não importa. É o helper que a
/// RFC-007 (regra 10) pede para o contrato de cor não apodrecer: qualquer
/// par de texto informativo novo pode ser checado contra um limiar antes
/// de entrar em produção.
#[cfg(test)]
pub fn contrast_ratio(a: Color, b: Color) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// limiar WCAG para texto normal (corpo de leitura contínua).
    const NORMAL_TEXT: f32 = 4.5;
    /// limiar WCAG para texto grande/negrito ou elemento de UI curto
    /// (rótulo, botão, medidor) — RFC-007, tabela da seção Problema.
    const LARGE_TEXT_OR_UI: f32 = 3.0;

    /// Pares (rótulo, texto, fundo, limiar) que a tela de duelo desenha
    /// de verdade como texto informativo, já com as correções da
    /// RFC-007 aplicadas. Decoração (bordas finas, divisórias) fica de
    /// fora de propósito — regra 10 e a tabela de riscos da RFC.
    const INFORMATIVE_PAIRS: &[(&str, Color, Color, f32)] = &[
        ("texto principal (PAPIRO) / PEDRA", PAPIRO, PEDRA, NORMAL_TEXT),
        ("texto principal (PAPIRO) / TUMBA", PAPIRO, TUMBA, NORMAL_TEXT),
        ("texto secundario (POEIRA) / PEDRA — N LINHAS, custo de comando (regra 4)", POEIRA, PEDRA, LARGE_TEXT_OR_UI),
        ("texto secundario (POEIRA) / TUMBA — numero de linha (regra 4)", POEIRA, TUMBA, NORMAL_TEXT),
        ("destaque/valor (OURO) / PEDRA", OURO, PEDRA, NORMAL_TEXT),
        ("destaque/valor (OURO) / TUMBA — golpe bonus no log", OURO, TUMBA, NORMAL_TEXT),
        ("informacao (ESCARAVELHO) / PEDRA", ESCARAVELHO, PEDRA, NORMAL_TEXT),
        ("informacao (ESCARAVELHO) / TUMBA — palavra-chave no editor (regra 2)", ESCARAVELHO, TUMBA, NORMAL_TEXT),
        ("token de valor (MUSGO) / TUMBA — literal no editor", MUSGO, TUMBA, NORMAL_TEXT),
        ("token de valor (MUSGO) / OK_BG — SINTAXE OK", MUSGO, OK_BG, NORMAL_TEXT),
        ("vida/sucesso (VIDA) / TUMBA — cura, ataque efetivo no log", VIDA, TUMBA, NORMAL_TEXT),
        ("vida/sucesso (VIDA) / OK_BG", VIDA, OK_BG, NORMAL_TEXT),
        ("dano/erro (SANGUE) / TUMBA — erro, contra-ataque no log", SANGUE, TUMBA, NORMAL_TEXT),
        ("dano/erro (SANGUE) / PEDRA", SANGUE, PEDRA, LARGE_TEXT_OR_UI),
        ("dano/erro (SANGUE) / DANGER_BG — texto de erro em BODY_MD (regra 6)", SANGUE, DANGER_BG, LARGE_TEXT_OR_UI),
        ("tag de fraqueza (PAPIRO) / AREIA_ESCURA (regra 6)", PAPIRO, AREIA_ESCURA, NORMAL_TEXT),
    ];

    #[test]
    fn informative_text_pairs_meet_wcag_contrast() {
        for (label, fg, bg, threshold) in INFORMATIVE_PAIRS {
            let ratio = contrast_ratio(*fg, *bg);
            assert!(ratio >= *threshold, "{label}: razao {ratio:.2} abaixo do limiar {threshold:.1}");
        }
    }

    #[test]
    fn contrast_ratio_is_order_independent_and_never_below_one() {
        assert!((contrast_ratio(PAPIRO, TUMBA) - contrast_ratio(TUMBA, PAPIRO)).abs() < 1e-6);
        assert!(contrast_ratio(PEDRA, PEDRA) >= 1.0);
    }
}

//! RFC-031: o jogo inteiro desenha em coordenadas fixas de 1280x720
//! (`config::WIDTH`/`HEIGHT`) — cada cena assume esse canvas. Antes desta
//! RFC, `window_conf()` pedia uma janela 1280x720 mas não travava o
//! redimensionamento (`Conf::window_resizable` é `true` por padrão no
//! macroquad), então redimensionar a janela do SO fazia
//! `screen_width()`/`screen_height()` mudarem sem que nenhum desenho
//! escalasse — resultado: UI sobreposta/cortada nos screenshots que
//! motivaram esta RFC.
//!
//! A solução (padrão "letterbox" do próprio macroquad, ver
//! `examples/letterbox.rs` do crate): todo o desenho de um frame vai para
//! um `render_target` fixo de 1280x720, e essa textura é depois desenhada
//! na janela real, escalada e centralizada preservando 16:9 — nunca
//! esticada. As cenas continuam desenhando em WIDTH/HEIGHT fixos, sem
//! saber que existe escala (não-objetivo explícito da RFC-031).
//!
//! `FilterMode::Nearest` em vez do `Linear` do exemplo oficial: o jogo é
//! pixel art (RFC-031, tabela de riscos) — `Linear` borraria o texto ao
//! escalar para cima.

use macroquad::prelude::*;

use crate::config::{HEIGHT, WIDTH};

/// Cria o render target de 1280x720 usado como canvas virtual. Chamado uma
/// única vez em `main()` — recriar por frame realocaria textura de vídeo
/// sem motivo.
pub fn make_render_target() -> RenderTarget {
    let target = render_target(WIDTH as u32, HEIGHT as u32);
    target.texture.set_filter(FilterMode::Nearest);
    target
}

/// Câmera que aponta todo o desenho do frame para dentro do render target,
/// mapeando 1:1 as coordenadas do canvas virtual (0,0)-(WIDTH,HEIGHT) —
/// é por isso que nenhuma cena precisa mudar: continuam desenhando nos
/// mesmos números de sempre.
pub fn virtual_camera(target: &RenderTarget) -> Camera2D {
    let mut camera = Camera2D::from_display_rect(Rect::new(0.0, 0.0, WIDTH, HEIGHT));
    camera.render_target = Some(target.clone());
    camera
}

/// Fator de escala do canvas virtual (1280x720) para a janela real,
/// preservando a proporção 16:9 — usa o menor dos dois eixos para nunca
/// cortar nem esticar (mesma fórmula do `examples/letterbox.rs` do
/// macroquad).
fn letterbox_scale() -> f32 {
    f32::min(screen_width() / WIDTH, screen_height() / HEIGHT)
}

/// Deslocamento (canto superior esquerdo do canvas escalado dentro da
/// janela real) que centraliza o canvas, deixando a sobra como barras
/// pretas nas bordas.
fn letterbox_offset(scale: f32) -> (f32, f32) {
    (
        (screen_width() - WIDTH * scale) * 0.5,
        (screen_height() - HEIGHT * scale) * 0.5,
    )
}

/// Desenha o render target na janela real, escalado e centralizado com
/// letterbox. Preto puro nas barras (`BLACK` do macroquad, não
/// `ui::theme`) — é fora do canvas do jogo, a paleta do jogo é para dentro
/// dele (RFC-031, tabela de riscos).
pub fn draw_letterboxed(target: &RenderTarget) {
    let scale = letterbox_scale();
    let (offset_x, offset_y) = letterbox_offset(scale);

    clear_background(BLACK);

    draw_texture_ex(
        &target.texture,
        offset_x,
        offset_y,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(WIDTH * scale, HEIGHT * scale)),
            // Render targets do macroquad ficam de cabeça para baixo por
            // convenção de coordenadas de textura OpenGL -- sem isto a
            // tela inteira apareceria invertida verticalmente.
            flip_y: true,
            ..Default::default()
        },
    );
}

/// Ponto único de conversão do mouse: `mouse_position()` do macroquad
/// devolve coordenadas da janela real, mas todo hit-test de cena
/// (`Rect::contains`, `Button::update_hover`) foi escrito assumindo
/// coordenadas do canvas virtual 1280x720. Sem esta conversão, clicar
/// num botão erraria a posição sempre que a janela real não for
/// exatamente 1280x720.
///
/// Desvio declarado da RFC-031: o macroquad 0.4.16 não expõe nenhuma API
/// pública para sobrescrever o estado global de mouse que
/// `mouse_position()` lê (`Context::mouse_position` é campo privado do
/// crate, só mutado pelo próprio tratamento de evento de janela) --
/// confirmado lendo `macroquad-0.4.16/src/input.rs` e `src/lib.rs`. Não
/// existe, portanto, um jeito de fazer todas as cenas passarem a ler
/// coordenadas de canvas virtual sem que cada uma troque a chamada
/// `mouse_position()` por `virtual_mouse_position()` -- é uma troca
/// mecânica de nome de função (nenhum número de coordenada/layout muda),
/// mas ainda assim toca `src/scenes/*.rs`/`src/ui/pause_menu.rs`, o que a
/// RFC pede para evitar. Ver nota de investigação para o
/// `[[product-manager]]`.
pub fn virtual_mouse_position() -> (f32, f32) {
    let scale = letterbox_scale();
    let (offset_x, offset_y) = letterbox_offset(scale);
    let (real_x, real_y) = mouse_position();
    ((real_x - offset_x) / scale, (real_y - offset_y) / scale)
}

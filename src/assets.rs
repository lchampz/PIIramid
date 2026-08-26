//! Carga única de todas as texturas e fontes do jogo, no boot.
//! Substitui os vários `al_load_bitmap`/`al_load_font` espalhados pelo C original.
//!
//! O pixel-art foi gerado por `tools/gen_assets.py` (sem depender de
//! nenhum serviço externo de geração de imagem). A tipografia — Press
//! Start 2P para títulos/números grandes, Silkscreen para o resto — veio
//! do layout em `PIIramid Layout.dc.html` e está em `assets/fonts/`
//! (Google Fonts, licença OFL, ver `assets/fonts/OFL-*.txt`).

use macroquad::prelude::*;

use crate::script::value::ItemKind;
use crate::world::entity::Kind;

pub struct Assets {
    pub bg_menu: Texture2D,
    pub bg_dungeon: Texture2D,

    pub sprite_player: Texture2D,
    pub sprite_mummy: Texture2D,
    pub sprite_zombie: Texture2D,
    pub sprite_beetle: Texture2D,
    pub sprite_sphinx: Texture2D,
    pub sprite_guardiao: Texture2D,
    pub sprite_sentinela: Texture2D,

    pub portrait_player: Texture2D,
    pub portrait_mummy: Texture2D,
    pub portrait_zombie: Texture2D,
    pub portrait_beetle: Texture2D,
    pub portrait_sphinx: Texture2D,
    pub portrait_guardiao: Texture2D,
    pub portrait_sentinela: Texture2D,

    pub icon_espada: Texture2D,
    pub icon_magia: Texture2D,
    pub icon_escudo: Texture2D,
    pub icon_pocao: Texture2D,

    pub tile_floor: Texture2D,
    pub tile_wall: Texture2D,

    /// Press Start 2P — títulos, números grandes, botões
    pub font_title: Font,
    /// Silkscreen regular — corpo de texto, listas, rótulos
    pub font_body: Font,
    /// Silkscreen bold — ênfase dentro de blocos de corpo
    pub font_body_bold: Font,
}

async fn load_pixel_texture(path: &str) -> Texture2D {
    let tex = load_texture(path)
        .await
        .unwrap_or_else(|e| panic!("falha ao carregar textura '{path}': {e}"));
    tex.set_filter(FilterMode::Nearest);
    tex
}

async fn load_font(path: &str) -> Font {
    load_ttf_font(path).await.unwrap_or_else(|e| panic!("falha ao carregar fonte '{path}': {e}"))
}

impl Assets {
    pub async fn load() -> Self {
        Assets {
            bg_menu: load_pixel_texture("./assets/bg_menu.png").await,
            bg_dungeon: load_pixel_texture("./assets/bg_dungeon.png").await,

            sprite_player: load_pixel_texture("./assets/sprite.png").await,
            sprite_mummy: load_pixel_texture("./assets/monsters/mummy.png").await,
            sprite_zombie: load_pixel_texture("./assets/monsters/zombie.png").await,
            sprite_beetle: load_pixel_texture("./assets/monsters/beetle.png").await,
            sprite_sphinx: load_pixel_texture("./assets/monsters/sphinx.png").await,
            sprite_guardiao: load_pixel_texture("./assets/monsters/guardiao.png").await,
            sprite_sentinela: load_pixel_texture("./assets/monsters/sentinela.png").await,

            portrait_player: load_pixel_texture("./assets/portraits/player.png").await,
            portrait_mummy: load_pixel_texture("./assets/portraits/mummy.png").await,
            portrait_zombie: load_pixel_texture("./assets/portraits/zombie.png").await,
            portrait_beetle: load_pixel_texture("./assets/portraits/beetle.png").await,
            portrait_sphinx: load_pixel_texture("./assets/portraits/sphinx.png").await,
            portrait_guardiao: load_pixel_texture("./assets/portraits/guardiao.png").await,
            portrait_sentinela: load_pixel_texture("./assets/portraits/sentinela.png").await,

            icon_espada: load_pixel_texture("./assets/icons/espada.png").await,
            icon_magia: load_pixel_texture("./assets/icons/magia.png").await,
            icon_escudo: load_pixel_texture("./assets/icons/escudo.png").await,
            icon_pocao: load_pixel_texture("./assets/icons/pocao.png").await,

            tile_floor: load_pixel_texture("./assets/tileset/chao1.png").await,
            tile_wall: load_pixel_texture("./assets/tileset/muro1.png").await,

            font_title: load_font("./assets/fonts/PressStart2P-Regular.ttf").await,
            font_body: load_font("./assets/fonts/Silkscreen-Regular.ttf").await,
            font_body_bold: load_font("./assets/fonts/Silkscreen-Bold.ttf").await,
        }
    }

    pub fn sprite_for(&self, kind: Kind) -> &Texture2D {
        match kind {
            Kind::Player => &self.sprite_player,
            Kind::Mummy => &self.sprite_mummy,
            Kind::Zombie => &self.sprite_zombie,
            Kind::Beetle => &self.sprite_beetle,
            Kind::Sphinx => &self.sprite_sphinx,
            Kind::Guardiao => &self.sprite_guardiao,
            Kind::Sentinela => &self.sprite_sentinela,
        }
    }

    pub fn icon_for(&self, kind: ItemKind) -> &Texture2D {
        match kind {
            ItemKind::Espada => &self.icon_espada,
            ItemKind::Magia => &self.icon_magia,
            ItemKind::Escudo => &self.icon_escudo,
            ItemKind::Pocao => &self.icon_pocao,
        }
    }

    pub fn portrait_for(&self, kind: Kind) -> &Texture2D {
        match kind {
            Kind::Player => &self.portrait_player,
            Kind::Mummy => &self.portrait_mummy,
            Kind::Zombie => &self.portrait_zombie,
            Kind::Beetle => &self.portrait_beetle,
            Kind::Sphinx => &self.portrait_sphinx,
            Kind::Guardiao => &self.portrait_guardiao,
            Kind::Sentinela => &self.portrait_sentinela,
        }
    }
}

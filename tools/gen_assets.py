"""Gerador procedural dos assets pixel-art do PIIramid.

Sem dependencia de API externa (sem Retro Diffusion / pixel-mcp disponiveis
nesta sessao) -- desenha tudo com formas simples em canvas pequeno e faz
upscale com NEAREST, o que da o look "pixel art" blocado de forma
deterministica e reproduzivel (`python tools/gen_assets.py`).

Paleta: deserto egipcio (areia, dourado, terracota, turquesa de acento).
"""

import os
import random

from PIL import Image, ImageDraw, ImageFilter, ImageFont

random.seed(7)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "assets")

# ---------------------------------------------------------------- paleta --

P = {
    "sky_top": (35, 20, 58, 255),
    "sky_mid": (120, 68, 92, 255),
    "sky_low": (232, 142, 92, 255),
    "sun": (255, 221, 133, 255),
    "sun_core": (255, 240, 190, 255),
    "sand_light": (232, 199, 126, 255),
    "sand": (212, 168, 90, 255),
    "sand_dark": (166, 124, 61, 255),
    "sand_shadow": (120, 88, 44, 255),
    "stone": (150, 132, 112, 255),
    "stone_mid": (112, 96, 80, 255),
    "stone_dark": (74, 60, 48, 255),
    "stone_shadow": (46, 36, 28, 255),
    "gold": (240, 196, 25, 255),
    "gold_dark": (176, 128, 16, 255),
    "turquoise": (56, 176, 160, 255),
    "turquoise_dark": (30, 110, 100, 255),
    "blood": (150, 40, 40, 255),
    "linen": (237, 224, 200, 255),
    "ink": (40, 28, 22, 255),
    "skin": (198, 156, 110, 255),
    "skin_dark": (150, 110, 75, 255),
    "robe": (54, 110, 140, 255),
    "robe_dark": (34, 76, 100, 255),
    "bandage": (224, 213, 184, 255),
    "bandage_dark": (176, 160, 124, 255),
    "bandage_shadow": (128, 112, 82, 255),
    "zombie": (108, 140, 90, 255),
    "zombie_dark": (66, 92, 56, 255),
    "zombie_rag": (58, 54, 40, 255),
    "beetle": (66, 52, 96, 255),
    "beetle_dark": (38, 30, 58, 255),
    "beetle_light": (120, 96, 168, 255),
    "sphinx_body": (214, 176, 96, 255),
    "sphinx_dark": (160, 124, 58, 255),
    "sphinx_gold": (240, 196, 25, 255),
    # RFC-008 -- Guardiao das Duas Chaves: pedra-bronze do "boss
    # cumulativo", mais escuro que a sphinx (tom mais antigo, mais
    # imponente) com a cabeca em dourado pra ecoar a dupla chave do adorno.
    "guardiao_body": (94, 78, 58, 255),
    "guardiao_dark": (58, 46, 34, 255),
    "guardiao_head": (206, 170, 88, 255),
    # RFC-012 -- Sentinela das Palavras Verdadeiras: tom pedra-ardosia fria
    # (diferente do bronze/dourado quente dos outros cinco) para destacar
    # que a fraqueza dele e sobre forma/nomeacao, nao sobre elemento ou
    # estado -- o adorno (headdress_nome_verdadeiro) reforca com uma
    # tabuleta gravada em vez de chifre/coroa.
    "sentinela_body": (70, 84, 92, 255),
    "sentinela_dark": (40, 50, 56, 255),
    "sentinela_head": (176, 200, 204, 255),
    # RFC-017 -- Necroguardiao (provisorio): fecha o ciclo de `invocar`
    # (RFC-004), entao o tom puxa pro roxo-necromante (diferente do
    # bronze/pedra-ardosia dos outros seis), com a cabeca em verde-espectral
    # pra ecoar os dois espiritos convocados do adorno.
    "necroguardiao_body": (72, 52, 84, 255),
    "necroguardiao_dark": (42, 28, 52, 255),
    "necroguardiao_head": (140, 200, 150, 255),
    "clear": (0, 0, 0, 0),
}


def canvas(w, h):
    return Image.new("RGBA", (w, h), P["clear"])


def up(img, scale):
    w, h = img.size
    return img.resize((w * scale, h * scale), Image.NEAREST)


def save(img, *path_parts):
    path = os.path.join(ASSETS, *path_parts)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path)
    print("wrote", os.path.relpath(path, ROOT))


def noise_dither(draw, box, base, alt, density=0.15):
    x0, y0, x1, y1 = box
    for y in range(y0, y1):
        for x in range(x0, x1):
            if random.random() < density:
                draw.point((x, y), fill=alt)


# ------------------------------------------------------------ backgrounds --

def gen_bg_menu():
    w, h = 320, 180
    img = canvas(w, h)
    d = ImageDraw.Draw(img)
    # ceu em faixas (gradiente quantizado)
    bands = [(P["sky_top"], 0, 70), (P["sky_mid"], 70, 105), (P["sky_low"], 105, 125)]
    for color, y0, y1 in bands:
        d.rectangle([0, y0, w, y1], fill=color)
    # sol
    d.ellipse([132, 78, 188, 134], fill=P["sun"])
    d.ellipse([146, 92, 174, 120], fill=P["sun_core"])
    # areia
    d.rectangle([0, 125, w, h], fill=P["sand"])
    noise_dither(d, (0, 125, w, h), P["sand"], P["sand_dark"], 0.08)
    d.rectangle([0, 150, w, h], fill=P["sand_dark"])
    noise_dither(d, (0, 150, w, h), P["sand_dark"], P["sand_shadow"], 0.1)

    def pyramid(cx, base_y, half_w, height, color, shadow):
        d.polygon([(cx - half_w, base_y), (cx, base_y - height), (cx + half_w, base_y)], fill=color)
        d.polygon([(cx, base_y - height), (cx + half_w, base_y), (cx + half_w * 0.35, base_y)], fill=shadow)

    pyramid(60, 150, 34, 46, P["sand_dark"], P["sand_shadow"])
    pyramid(230, 150, 40, 58, P["sand_dark"], P["sand_shadow"])
    pyramid(160, 148, 52, 78, P["sand"], P["sand_dark"])
    pyramid(160, 148, 22, 34, P["gold_dark"], P["stone_shadow"])

    save(up(img, 4), "bg_menu.png")


def gen_bg_dungeon():
    w, h = 320, 180
    img = canvas(w, h)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, w, h], fill=P["stone_mid"])
    # fileiras de blocos
    brick_h = 10
    for row, y in enumerate(range(0, h, brick_h)):
        offset = 0 if row % 2 == 0 else 12
        for x in range(-offset, w, 24):
            d.rectangle([x, y, x + 22, y + brick_h - 1], outline=P["stone_dark"], width=1)
    noise_dither(d, (0, 0, w, h), P["stone_mid"], P["stone_shadow"], 0.05)
    # piso
    d.rectangle([0, h - 40, w, h], fill=P["sand_dark"])
    noise_dither(d, (0, h - 40, w, h), P["sand_dark"], P["sand_shadow"], 0.12)
    d.line([0, h - 40, w, h - 40], fill=P["stone_shadow"], width=2)
    # tochas
    for tx in (28, w - 28):
        d.rectangle([tx - 2, h - 90, tx + 2, h - 55], fill=P["stone_dark"])
        d.polygon([(tx - 6, h - 100), (tx, h - 118), (tx + 6, h - 100)], fill=P["gold"])
        d.polygon([(tx - 3, h - 100), (tx, h - 108), (tx + 3, h - 100)], fill=P["sun_core"])
    # vinheta leve nas bordas
    vign = Image.new("L", (w, h), 0)
    vd = ImageDraw.Draw(vign)
    vd.rectangle([0, 0, w, h], fill=0)
    vd.rectangle([10, 10, w - 10, h - 10], fill=40)
    vign = vign.filter(ImageFilter.GaussianBlur(12))
    dark = Image.new("RGBA", (w, h), P["stone_shadow"])
    img = Image.composite(img, dark, vign.point(lambda p: 255 - p))

    save(up(img, 4), "bg_dungeon.png")


# ----------------------------------------------------------------- tiles --

def gen_tile_floor():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, s, s], fill=P["sand"])
    noise_dither(d, (0, 0, s, s), P["sand"], P["sand_dark"], 0.18)
    d.point([(1, 1), (6, 2), (3, 6)], fill=P["sand_light"])
    d.rectangle([0, 0, s - 1, s - 1], outline=P["sand_shadow"])
    save(up(img, 4), "tileset", "chao1.png")


def gen_tile_wall():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, s, s], fill=P["stone"])
    d.rectangle([0, 0, s - 1, 3], fill=P["stone_mid"])
    d.line([0, 4, s, 4], fill=P["stone_dark"], width=1)
    d.line([4, 0, 4, 3], fill=P["stone_dark"], width=1)
    d.line([2, 5, 2, s], fill=P["stone_dark"], width=1)
    d.line([6, 5, 6, s], fill=P["stone_dark"], width=1)
    d.rectangle([0, 0, s - 1, s - 1], outline=P["stone_shadow"])
    save(up(img, 4), "tileset", "muro1.png")


# ------------------------------------------------------------- criaturas --

DOWN, UP, LEFT, RIGHT = 0, 1, 2, 3


def draw_humanoid(d, direction, frame, skin, skin_dark, robe, robe_dark, headwear=None, arms_out=False):
    cx, cy = 8, 8
    swing = [0, 1, 0, -1][frame]

    # pernas
    lx0, lx1 = cx - 3, cx + 1
    ly = 11
    d.rectangle([lx0, ly, lx0 + 2, ly + 3 + max(0, swing)], fill=skin_dark)
    d.rectangle([lx1, ly, lx1 + 2, ly + 3 + max(0, -swing)], fill=skin_dark)

    # robe/torso
    d.rectangle([cx - 4, 5, cx + 4, 12], fill=robe)
    d.line([cx - 4, 5, cx + 4, 5], fill=robe_dark)
    d.rectangle([cx - 4, 10, cx + 4, 12], fill=robe_dark)

    # bracos
    if arms_out:
        d.rectangle([cx - 6, 6, cx - 5, 9], fill=skin_dark)
        d.rectangle([cx + 5, 6, cx + 6, 9], fill=skin_dark)
    else:
        a_swing = [0, -1, 0, 1][frame]
        d.rectangle([cx - 5, 7 + max(0, a_swing), cx - 4, 9 + max(0, a_swing)], fill=skin_dark)
        d.rectangle([cx + 4, 7 + max(0, -a_swing), cx + 5, 9 + max(0, -a_swing)], fill=skin_dark)

    # cabeca
    d.rectangle([cx - 3, 1, cx + 3, 6], fill=skin)
    d.rectangle([cx - 3, 1, cx + 3, 2], fill=skin_dark)

    # rosto conforme direcao
    if direction == DOWN:
        d.point([(cx - 1, 4), (cx + 1, 4)], fill=P["ink"])
    elif direction == LEFT:
        d.point([(cx - 2, 4)], fill=P["ink"])
    elif direction == RIGHT:
        d.point([(cx + 2, 4)], fill=P["ink"])
    # UP (de costas): sem rosto

    if headwear:
        headwear(d, cx, cy, direction)


def headwear_headband(d, cx, cy, direction):
    d.line([cx - 3, 2, cx + 3, 2], fill=P["turquoise"], width=1)


def headwear_hood(d, cx, cy, direction):
    d.rectangle([cx - 4, 0, cx + 4, 3], fill=P["zombie_rag"])


def draw_mummy(d, direction, frame):
    cx = 8
    swing = [0, 1, 0, -1][frame]
    # pernas enfaixadas, andar rigido
    d.rectangle([cx - 3, 11, cx - 1, 14 + max(0, swing)], fill=P["bandage_dark"])
    d.rectangle([cx + 1, 11, cx + 3, 14 + max(0, -swing)], fill=P["bandage_dark"])
    # torso
    d.rectangle([cx - 4, 4, cx + 4, 12], fill=P["bandage"])
    for i, y in enumerate(range(5, 12, 2)):
        color = P["bandage_dark"] if i % 2 == 0 else P["bandage_shadow"]
        d.line([cx - 4, y, cx + 4, y], fill=color)
    # bracos estendidos (pose classica de mumia)
    d.rectangle([cx - 7, 6, cx - 4, 7], fill=P["bandage_dark"])
    d.rectangle([cx + 4, 6, cx + 7, 7], fill=P["bandage_dark"])
    # cabeca enfaixada
    d.rectangle([cx - 3, 0, cx + 3, 5], fill=P["bandage"])
    d.line([cx - 3, 2, cx + 3, 2], fill=P["bandage_shadow"])
    if direction == DOWN:
        d.point([(cx - 1, 3), (cx + 1, 3)], fill=P["ink"])
    elif direction == LEFT:
        d.point([(cx - 2, 3)], fill=P["ink"])
    elif direction == RIGHT:
        d.point([(cx + 2, 3)], fill=P["ink"])


def draw_creature(d, direction, frame, body, body_dark, head, size_h=6, legs=4, headdress=None, round_body=True):
    cx, cy = 8, 9
    swing = [0, 1, 0, -1][frame]
    body_w = 6
    top = cy - size_h // 2
    bottom = cy + size_h // 2

    # pernas
    leg_positions = [-4, -1, 2, 5][:legs] if legs == 4 else [-3, 3]
    for i, lx in enumerate(leg_positions):
        off = swing if i % 2 == 0 else -swing
        d.rectangle([cx + lx, bottom - 1, cx + lx + 1, bottom + 2 + max(0, off)], fill=body_dark)

    # corpo
    if round_body:
        d.ellipse([cx - body_w, top, cx + body_w, bottom], fill=body)
        d.ellipse([cx - body_w, top, cx + body_w, top + 3], fill=body_dark)
    else:
        d.rectangle([cx - body_w, top, cx + body_w, bottom], fill=body)
        d.rectangle([cx - body_w, top, cx + body_w, top + 2], fill=body_dark)

    # cabeca
    d.ellipse([cx + body_w - 3, top - 2, cx + body_w + 3, top + 4], fill=head)
    d.point([(cx + body_w, top + 1)], fill=P["ink"])

    if headdress:
        headdress(d, cx, top)


def headdress_sphinx(d, cx, top):
    d.polygon([(cx + 4, top - 2), (cx + 9, top - 8), (cx + 11, top - 1)], fill=P["gold"])
    d.polygon([(cx + 4, top - 2), (cx + 2, top - 7), (cx + 6, top - 3)], fill=P["gold_dark"])


def headdress_antennae(d, cx, top):
    d.line([(cx + body_w_g, top - 1), (cx + body_w_g + 3, top - 5)], fill=P["ink"])
    d.line([(cx + body_w_g, top), (cx + body_w_g + 4, top - 3)], fill=P["ink"])


def headdress_duplo_selo(d, cx, top):
    """Duas chaves cruzadas sobre a cabeca (RFC-008): a fraqueza do
    Guardiao e a composicao das duas condicoes que os monstros anteriores
    cobravam isoladas (guarda do escaravelho, inspecao da esfinge) -- o
    adorno mostra literalmente as "duas chaves" se cruzando, com o selo
    (ponto turquesa) no encontro delas."""
    d.line([(cx - 5, top), (cx - 1, top - 6)], fill=P["gold"])
    d.line([(cx + 1, top - 6), (cx + 5, top)], fill=P["gold_dark"])
    d.ellipse([cx - 6, top - 1, cx - 4, top + 1], outline=P["gold"])
    d.ellipse([cx + 4, top - 1, cx + 6, top + 1], outline=P["gold_dark"])
    d.point([(cx, top - 6)], fill=P["turquoise"])


def headdress_nome_verdadeiro(d, cx, top):
    """Tabuleta gravada sobre a cabeca (RFC-012): a fraqueza do Sentinela
    julga se o golpe foi *nomeado* numa funcao -- o adorno mostra
    literalmente uma tabuleta com um traco (o "nome" gravado nela), em vez
    de um elemento ou par de simbolos como os monstros anteriores."""
    d.rectangle([cx - 4, top - 6, cx + 4, top - 1], fill=P["stone"])
    d.rectangle([cx - 4, top - 6, cx + 4, top - 5], fill=P["stone_dark"])
    d.line([(cx - 2, top - 3), (cx + 2, top - 3)], fill=P["turquoise"])


def headdress_invocacao_dupla(d, cx, top):
    """Dois cranios espectrais flutuando sobre a cabeca (RFC-017): a
    fraqueza do Necroguardiao so cede depois de 2 invocacoes no turno -- o
    adorno mostra literalmente os dois espiritos convocados, um de cada
    lado, em vez de um par de simbolos cruzados (Aker) ou uma tabuleta
    (Apagado)."""
    d.ellipse([cx - 7, top - 6, cx - 3, top - 2], outline=P["turquoise"])
    d.ellipse([cx + 3, top - 6, cx + 7, top - 2], outline=P["turquoise"])
    d.point([(cx - 5, top - 4)], fill=P["turquoise"])
    d.point([(cx + 5, top - 4)], fill=P["turquoise"])


body_w_g = 6


def build_sheet(draw_frame_fn):
    """4 direcoes (linhas) x 4 frames (colunas), celula 16x16 -> upscale x4."""
    cell = 16
    sheet = canvas(cell * 4, cell * 4)
    for direction in (DOWN, UP, LEFT, RIGHT):
        for frame in range(4):
            cell_img = canvas(cell, cell)
            d = ImageDraw.Draw(cell_img)
            draw_frame_fn(d, direction, frame)
            sheet.paste(cell_img, (frame * cell, direction * cell), cell_img)
    return up(sheet, 4)


def gen_player():
    def frame(d, direction, f):
        draw_humanoid(d, direction, f, P["skin"], P["skin_dark"], P["robe"], P["robe_dark"], headwear_headband)
    save(build_sheet(frame), "sprite.png")


def gen_zombie():
    def frame(d, direction, f):
        draw_humanoid(d, direction, f, P["zombie"], P["zombie_dark"], P["zombie_rag"], P["stone_shadow"], headwear_hood)
    save(build_sheet(frame), "monsters", "zombie.png")


def gen_mummy():
    def frame(d, direction, f):
        draw_mummy(d, direction, f)
    save(build_sheet(frame), "monsters", "mummy.png")


def gen_beetle():
    def frame(d, direction, f):
        draw_creature(d, direction, f, P["beetle"], P["beetle_dark"], P["beetle_light"], size_h=6, legs=4, headdress=headdress_antennae)
    save(build_sheet(frame), "monsters", "beetle.png")


def gen_sphinx():
    def frame(d, direction, f):
        draw_creature(d, direction, f, P["sphinx_body"], P["sphinx_dark"], P["sphinx_gold"], size_h=7, legs=4, headdress=headdress_sphinx, round_body=False)
    save(build_sheet(frame), "monsters", "sphinx.png")


def gen_guardiao():
    # RFC-008: "boss cumulativo" depois dos 4 -- corpo maior (size_h=8,
    # o mais alto do bestiario) e quadrado (round_body=False, como a
    # esfinge, pra leitura de "estatua imponente"), reaproveitando
    # draw_creature como os outros dois monstros nao-humanoides.
    def frame(d, direction, f):
        draw_creature(d, direction, f, P["guardiao_body"], P["guardiao_dark"], P["guardiao_head"], size_h=8, legs=4, headdress=headdress_duplo_selo, round_body=False)
    save(build_sheet(frame), "monsters", "guardiao.png")


def gen_sentinela():
    # RFC-012: mesmo padrao dos monstros nao-humanoides anteriores
    # (draw_creature) -- corpo medio (size_h=7, entre beetle e guardiao),
    # quadrado (round_body=False, "estatua" como esfinge/guardiao) para
    # ler como guardiao de ritual, com a tabuleta no lugar do
    # chifre/coroa.
    def frame(d, direction, f):
        draw_creature(d, direction, f, P["sentinela_body"], P["sentinela_dark"], P["sentinela_head"], size_h=7, legs=4, headdress=headdress_nome_verdadeiro, round_body=False)
    save(build_sheet(frame), "monsters", "sentinela.png")


def gen_necroguardiao():
    # RFC-017: setimo monstro, mesmo padrao dos nao-humanoides anteriores
    # (draw_creature) -- corpo medio (size_h=7, igual ao sentinela),
    # quadrado (round_body=False, "estatua" como os demais guardioes) com
    # o adorno de dois espiritos convocados no lugar de chifre/tabuleta.
    def frame(d, direction, f):
        draw_creature(d, direction, f, P["necroguardiao_body"], P["necroguardiao_dark"], P["necroguardiao_head"], size_h=7, legs=4, headdress=headdress_invocacao_dupla, round_body=False)
    save(build_sheet(frame), "monsters", "necroguardiao.png")


# ------------------------------------------------------------- retratos --

def gen_portrait(name, draw_fn, bg=P["stone_mid"]):
    s = 48
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, s, s], fill=bg)
    noise_dither(d, (0, 0, s, s), bg, P["stone_dark"], 0.06)
    # moldura
    d.rectangle([0, 0, s - 1, s - 1], outline=P["gold_dark"], width=2)
    inner = canvas(16, 16)
    di = ImageDraw.Draw(inner)
    draw_fn(di)
    inner = inner.resize((36, 36), Image.NEAREST) # type: ignore
    img.paste(inner, (6, 8), inner)
    save(up(img, 4), "portraits", f"{name}.png")


def gen_portraits():
    gen_portrait("player", lambda d: draw_humanoid(d, DOWN, 0, P["skin"], P["skin_dark"], P["robe"], P["robe_dark"], headwear_headband))
    gen_portrait("mummy", lambda d: draw_mummy(d, DOWN, 0), bg=P["sand_shadow"])
    gen_portrait("zombie", lambda d: draw_humanoid(d, DOWN, 0, P["zombie"], P["zombie_dark"], P["zombie_rag"], P["stone_shadow"], headwear_hood), bg=P["stone_shadow"])
    gen_portrait("beetle", lambda d: draw_creature(d, DOWN, 0, P["beetle"], P["beetle_dark"], P["beetle_light"], headdress=headdress_antennae), bg=P["stone_dark"])
    gen_portrait("sphinx", lambda d: draw_creature(d, DOWN, 0, P["sphinx_body"], P["sphinx_dark"], P["sphinx_gold"], headdress=headdress_sphinx, round_body=False), bg=P["sand_shadow"])
    gen_portrait("guardiao", lambda d: draw_creature(d, DOWN, 0, P["guardiao_body"], P["guardiao_dark"], P["guardiao_head"], size_h=8, headdress=headdress_duplo_selo, round_body=False), bg=P["stone_shadow"])
    gen_portrait("sentinela", lambda d: draw_creature(d, DOWN, 0, P["sentinela_body"], P["sentinela_dark"], P["sentinela_head"], size_h=7, headdress=headdress_nome_verdadeiro, round_body=False), bg=P["stone_shadow"])
    gen_portrait("necroguardiao", lambda d: draw_creature(d, DOWN, 0, P["necroguardiao_body"], P["necroguardiao_dark"], P["necroguardiao_head"], size_h=7, headdress=headdress_invocacao_dupla, round_body=False), bg=P["stone_shadow"])


# ------------------------------------------------------------------ icons --

def gen_icon_espada():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.line([(1, 6), (6, 1)], fill=P["stone"], width=2)
    d.point([(6, 1)], fill=P["linen"])
    d.line([(1, 4), (3, 6)], fill=P["gold_dark"], width=1)
    d.point([(1, 6)], fill=P["gold"])
    save(up(img, 4), "icons", "espada.png")


def gen_icon_magia():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.point([(4, 1), (4, 7), (1, 4), (7, 4), (2, 2), (6, 2), (2, 6), (6, 6)], fill=P["turquoise"])
    d.rectangle([3, 3, 4, 4], fill=P["linen"])
    save(up(img, 4), "icons", "magia.png")


def gen_icon_escudo():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.polygon([(1, 1), (6, 1), (6, 4), (3, 7), (1, 4)], fill=P["stone"])
    d.polygon([(1, 1), (6, 1), (6, 2), (1, 2)], fill=P["gold"])
    d.point([(3, 3), (3, 4)], fill=P["gold_dark"])
    save(up(img, 4), "icons", "escudo.png")


def gen_icon_pocao():
    s = 8
    img = canvas(s, s)
    d = ImageDraw.Draw(img)
    d.rectangle([3, 0, 4, 1], fill=P["stone"])
    d.polygon([(2, 2), (5, 2), (6, 6), (1, 6)], fill=P["turquoise"])
    d.line([(2, 2), (5, 2)], fill=P["linen"])
    d.point([(3, 4), (4, 5)], fill=P["sun_core"])
    save(up(img, 4), "icons", "pocao.png")


# RFC-013 -- selo partido, compartilhado entre postura_aberta e
# fraqueza_guarda: mesma geometria de "escudo quebrado" usada nas duas
# familias, para nao duplicar o desenho (ver ASSETS-especificacoes.md).
def draw_selo_partido(d):
    d.polygon([(0, 1), (4, 1), (4, 4), (2, 6), (0, 4)], fill=P["stone"])
    d.polygon([(4, 2), (7, 2), (7, 5), (4, 7), (4, 4)], fill=P["stone_dark"])
    d.line([(4, 1), (4, 7)], fill=P["blood"], width=1)
    d.point([(6, 6)], fill=P["stone_dark"])


def gen_icon_postura():
    def guarda(d):
        d.polygon([(1, 1), (6, 1), (6, 4), (3, 7), (1, 4)], fill=P["stone"])
        d.rectangle([1, 1, 6, 2], fill=P["gold"])
        d.point([(3, 3), (3, 4)], fill=P["gold_dark"])
        d.line([(1, 1), (6, 1)], fill=P["linen"])

    for name, fn in (("guarda", guarda), ("aberta", draw_selo_partido)):
        img = canvas(8, 8)
        fn(ImageDraw.Draw(img))
        save(up(img, 4), "icons", f"postura_{name}.png")


def gen_icon_fraqueza():
    def elemento(d):
        d.polygon([(2, 7), (6, 7), (4, 1)], fill=P["gold"])
        d.polygon([(3, 6), (5, 6), (4, 3)], fill=P["sun_core"])
        d.point([(3, 7), (5, 7)], fill=P["stone_dark"])

    def eficiencia(d):
        d.polygon([(1, 1), (6, 1), (4, 4), (6, 7), (1, 7), (3, 4)], outline=P["stone_dark"])
        d.polygon([(2, 6), (5, 6), (4, 5), (3, 5)], fill=P["sand_light"])
        d.point([(4, 4)], fill=P["sand_light"])

    def inspecao(d):
        d.line([(1, 4), (4, 2), (7, 4)], fill=P["ink"], width=1)
        d.line([(2, 5), (6, 5)], fill=P["ink"], width=1)
        d.point([(4, 3)], fill=P["linen"])

    variants = {
        "elemento": elemento,
        "guarda": draw_selo_partido,
        "eficiencia": eficiencia,
        "inspecao": inspecao,
    }
    for name, fn in variants.items():
        img = canvas(8, 8)
        fn(ImageDraw.Draw(img))
        save(up(img, 4), "icons", f"fraqueza_{name}.png")


# ------------------------------------------------------------------- ui --
# Nao ha mais gen_button(): os botoes agora sao desenhados em Rust
# (ui/button.rs) com retangulo+borda, sem depender de textura, para bater
# com o estilo do layout em `PIIramid Layout.dc.html`.


def gen_logo():
    w, h = 780, 160
    img = canvas(w, h)
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype(os.path.join(ASSETS, "font.ttf"), 100)
    text = "PIIRAMID"
    bbox = d.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    tx, ty = (w - tw) // 2 - bbox[0], (h - th) // 2 - bbox[1]
    outline = P["stone_shadow"]
    for dx in (-4, 0, 4):
        for dy in (-4, 0, 4):
            if dx or dy:
                d.text((tx + dx, ty + dy), text, font=font, fill=outline)
    d.text((tx, ty - 4), text, font=font, fill=P["gold_dark"])
    d.text((tx, ty), text, font=font, fill=P["gold"])
    save(img, "logo.png")


# ------------------------------------------------------------------ main --

def main():
    gen_bg_menu()
    gen_bg_dungeon()
    gen_tile_floor()
    gen_tile_wall()
    gen_player()
    gen_zombie()
    gen_mummy()
    gen_beetle()
    gen_sphinx()
    gen_guardiao()
    gen_sentinela()
    gen_necroguardiao()
    gen_portraits()
    gen_icon_espada()
    gen_icon_magia()
    gen_icon_escudo()
    gen_icon_pocao()
    gen_icon_postura()
    gen_icon_fraqueza()
    gen_logo()


if __name__ == "__main__":
    main()

# PIIramid

Um jogo onde você derrota monstros escrevendo o algoritmo certo, não a
conta certa. Inspirado em *The Farmer Was Replaced* (você escreve código
de verdade e ele executa) e *Stardew Valley* (pixel-art top-down).

Reescrito do zero em **Rust** — o jogo original era C + Allegro5, com
combate por perguntas de matemática. Aqui, cada monstro tem uma fraqueza
*algorítmica*, e você vence escrevendo um script de pseudo-código que a
explora dentro de um orçamento de ciclos. Visual redesenhado a partir do
protótipo em `PIIramid Layout.dc.html` — ver
`C:\docs\Piiramid\Roadmap.md` para o roadmap e as RFCs de próximos passos.

## Rodando

```bash
cargo run --release
```

Requer o [Rust](https://rustup.rs/) estável (`cargo`/`rustc`). Sem
dependências de sistema — o motor gráfico ([macroquad](https://github.com/not-fl3/macroquad))
não precisa de SDK nativo instalado.

Controles no mapa: `WASD` ou setas para andar. Encoste em um monstro para
entrar em duelo.

## Telas

- **Menu** — nova expedição, continuar, grimório, guia de estilo, sair.
- **Batalha (duelo)** — editor com destaque de sintaxe, paleta de
  comandos clicável, arena com retratos animados e dano flutuante,
  dossiê do monstro (postura, carga do golpe especial, fraqueza), log
  de eventos.
- **Fim de combate** — estatísticas reais do duelo (turnos, vida
  restante, resultado).
- **Grimório** — inventário/scripts salvos; **mockado por decisão de
  escopo** (ver RFC-002 em `C:\docs\Piiramid\RFCs\`), ainda sem sistema
  de itens de verdade por trás.
- **Guia de estilo** — referência visual navegável (paleta, tipografia,
  botões, barras), porta ~1:1 do mockup original.

## A linguagem do duelo

Você escreve um script no editor e aperta **Executar** (ou `F5`). O
interpretador roda o script instrução por instrução, debitando **ciclos**
de um orçamento — o monstro contra-ataca se o orçamento estourar (um
golpe maior se a carga dele estiver cheia), e você ganha um golpe bônus
se sobrar ciclo no fim. Script enxuto vence; loop desperdiçado perde.

Dois estilos de bloco são aceitos, e podem ser misturados no mesmo script
(chaves podem aparecer dentro de um bloco indentado; o contrário não):

```python
# por indentação
inspecionar()

while inimigo.vida > 0:
    if inimigo.postura == "guarda":
        defender(escudo.Bronze)
    else:
        atacar(magia.Fogo)
```

```c
// por chaves
inspecionar()

while inimigo.vida > 0 {
    if inimigo.postura == "guarda" {
        defender(escudo.Bronze)
    } else {
        atacar(magia.Fogo)
    }
}
```

Argumentos de item usam **acesso por enum**: `magia.Fogo` em vez de uma
string solta (`magia["fogo"]` ainda funciona — as duas formas são
equivalentes — mas o editor colore `Fogo`/`Bronze`/`Vida` como valores de
enum, não como identificadores comuns).

### Referência

| | |
|---|---|
| `if cond: / cond {` … `else:` / `else {` | condicional |
| `while cond:` / `while cond {` | laço condicional |
| `for i in a..b:` / `for i in a..b {` | laço por intervalo |
| `x = expr` | atribuição de variável |
| `==` `!=` `<` `>` `<=` `>=` | comparação |
| `and`/`e`, `or`/`ou`, `not`/`nao` | lógicos |
| `+` `-` `*` `/` `%` | aritmética |
| `#` ou `//` | comentário até o fim da linha |

Funções nativas (cada uma custa ciclos do orçamento do turno):

| Chamada | Ciclos | Efeito |
|---|---|---|
| `atacar(item)` | 2 | causa dano; efetivo ou não depende da fraqueza do monstro |
| `defender(item)` | 1 | reduz pela metade o próximo contra-ataque |
| `inspecionar()` | 3 | revela a fraqueza de monstros que a escondem |
| `curar(item)` | 4 | recupera vida do jogador |
| `esperar()` | 1 | passa o ciclo sem agir |

Coleções indexáveis: `espada.Nome`, `magia.Elemento`, `escudo.Nome`,
`pocao.Nome` (ou `espada["nome"]` com aspas, equivalente). Campos de
estado: `inimigo.vida`, `inimigo.postura`, `eu.vida`, `eu.ciclos`.

Cada `while`/`for` cobra 1 ciclo por verificação de condição (inclusive a
que encerra o laço), então um algoritmo O(n²) custa muito mais que um
O(n) com o mesmo corpo — é aí que "o melhor algoritmo ganha".

### A carga do inimigo

Cada monstro acumula **carga** a cada turno (`+7`, teto em `20`). Se o
orçamento de ciclos do jogador estourar enquanto a carga está cheia, o
contra-ataque vira um **golpe especial** (mais dano) e a carga zera — a
barra de intenção na arena mostra qual dos dois vai acontecer *antes* do
jogador executar o script.

## Estrutura do projeto

```
src/
  main.rs         máquina de estados (Menu / Overworld / GameOver / Grimoire / StyleGuide)
  config.rs       constantes de janela
  assets.rs       carga única de texturas e fontes
  scenes/         cada cena do jogo
  world/          mapa em tiles e entidades (jogador/monstro)
  monsters/       catálogo de monstros, fraquezas e carga/golpe especial
  ui/             botão (sem textura), tema/paleta, editor de código
  script/         lexer, parser, VM e API do pseudo-código
```

`script/` é lógica pura (sem gráficos) e tem cobertura de testes própria:

```bash
cargo test
```

## Assets

O pixel-art de fundo/tileset/sprites/ícones (`assets/*.png`) é gerado por
código, sem nenhum asset externo —
`tools/gen_assets.py` desenha formas simples em canvas pequeno e faz
upscale com filtro `NEAREST`, o que dá o visual "pixel art" de forma
determinística e reproduzível.

```bash
python tools/gen_assets.py
```

Requer `pip install pillow numpy`. Reexecutar regenera os PNGs em
`assets/` (backgrounds, tileset, sprites de jogador/monstros, retratos do
duelo, ícones de item) a partir do zero.

As fontes (`assets/fonts/`) são baixadas do Google Fonts —
**Press Start 2P** (títulos) e **Silkscreen** (corpo), licença OFL (ver
`assets/fonts/OFL-*.txt`), com cobertura completa de acentuação PT-BR.

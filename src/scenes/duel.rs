//! A tela de duelo — porta de `PIIramid Layout.dc.html` (tela "Batalha"):
//! barra superior com câmara/turno/ciclos, editor com destaque de sintaxe
//! e paleta de comandos clicável, arena com retratos animados e dano
//! flutuante, dossiê do monstro com tags de fraqueza e barra de
//! intenção/carga, log de eventos colorido por categoria.

use std::cell::RefCell;
use std::collections::HashMap;

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::{HEIGHT, WIDTH};
use crate::inventory::{SaveData, SavedScript};
use crate::monsters::{MonsterState, Weakness};
use crate::script::error::ScriptError;
use crate::script::parser;
use crate::screen_scale::virtual_mouse_position;
use crate::script::rehearsal::{self, RehearsalEnd};
use crate::script::value::{ItemKind, Value};
use crate::script::vm::{self, TurnEvent, TurnResult};
use crate::ui::button::{Button, ButtonStyle};
use crate::ui::code_editor::CodeEditor;
use crate::ui::theme;
use crate::world::entity::{Entity, Kind};

const EVENT_TICK_SECONDS: f32 = 0.55;
const TOP_BAR_H: f32 = 62.0;
const EDITOR_W: f32 = 460.0;
const SIDE_W: f32 = 300.0;
const ARENA_X: f32 = EDITOR_W;
const ARENA_W: f32 = WIDTH - EDITOR_W - SIDE_W;

const EDITOR_BOX_Y: f32 = TOP_BAR_H + 10.0;
const EDITOR_BOX_H: f32 = 300.0;
// RFC-032 regra 1: a barra superior do editor virou 2 linhas em vez de 1.
// Antes, "turno.pii" + "{N} LINHAS" + LIMPAR/SALVAR/CARREGAR disputavam os
// mesmos 32px de altura -- com N de 3 dígitos "{N} LINHAS" cresce o
// suficiente pra encostar em CARREGAR (achado do usuário, ver RFC). Uma
// segunda linha elimina a disputa horizontal por construção: linha 1
// (nome do arquivo + contagem de linhas) e linha 2 (botões) nunca
// compartilham eixo X com o outro grupo, então não há par de elementos
// dos dois grupos que possa colidir, qualquer que seja a largura do texto.
const EDITOR_HEADER_H: f32 = 64.0;
/// Deslocamento em Y (a partir de `EDITOR_BOX_Y`) onde a 2a linha da barra
/// (os 3 botões) começa -- mesma proporção 5px topo / 1px fundo que a
/// barra de 1 linha original usava dentro do seu slot de 32px, agora
/// aplicada ao slot de 32px da 2a linha (`EDITOR_HEADER_H - 32.0`).
const EDITOR_HEADER_BUTTONS_Y: f32 = EDITOR_BOX_Y + 32.0 + 5.0;
const COMMAND_PANEL_Y: f32 = EDITOR_BOX_Y + EDITOR_BOX_H + 12.0;
const COMMAND_ROW_H: f32 = 32.0;
const COMMAND_ROW_GAP: f32 = 6.0;
const COMMAND_ROWS: f32 = 4.0;
const COMMAND_PANEL_H: f32 = 26.0 + COMMAND_ROWS * (COMMAND_ROW_H + COMMAND_ROW_GAP) - COMMAND_ROW_GAP;
const BUTTONS_Y: f32 = COMMAND_PANEL_Y + COMMAND_PANEL_H + 12.0;

pub enum DuelOutcome {
    Won,
    Lost,
    Fled,
}

enum Phase {
    Writing,
    Executing { result: TurnResult, index: usize, timer: f32 },
    Error(ScriptError),
}

/// RFC-018: resultado da validação ao vivo (parse real + passada de
/// validação real da VM) do texto atual do editor, recalculado a cada
/// frame em `Phase::Writing`. Não é `Phase` de propósito — ao contrário de
/// `Phase::Error`, isto nunca trava a edição nem substitui o resultado de
/// um EXECUTAR de verdade; é só o que a barra de sintaxe/ciclos mostra
/// *antes* do jogador apertar EXECUTAR. Cacheado em `DuelScene` porque
/// `draw()` não tem acesso a `SaveData`/`MonsterState` mutável o bastante
/// pra recalcular na hora de desenhar (ver `update()`).
enum LiveCheck {
    /// `parser::parse` ou a passada de validação da VM rejeitaram o texto
    /// atual — o erro é real, mesmo formato que `Phase::Error` já usa.
    Invalid(ScriptError),
    /// Parseou e validou. `truncated` reflete só se o orçamento estourou
    /// na validação — isso não é erro de sintaxe (regra 4 da RFC-018),
    /// só faz a barra de ciclos ficar no alerta que `over` já define.
    Valid { cycles_used: u32, truncated: bool },
}

struct HitPopup {
    value: i32,
    special: bool,
    timer: f32,
}

// RFC-020: reação visual de dano nos retratos — mesma técnica do idle bob
// (`hero_bob`/`foe_bob` em `draw_arena`), sem sprite-sheet novo. Dois tipos
// de reação, independentes, porque cada um nasce de um evento diferente:
// `HitReaction` ("eu tomei o golpe": recuo em shake + tingimento de sangue)
// e `LungeReaction` ("eu desferi o golpe": avanço em direção ao alvo).
//
// Números abaixo são chute informado do gamedev, não do designer (não há
// designer disponível agora — RFC-020 pede pra documentar a justificativa
// e marcar como ajustável). // TODO(designer): revisar todos os números
// desta seção.

/// 3 meios-ciclos de seno = vai-volta-vai — lê como "impacto" em vez de um
/// deslize único pro lado, dentro da janela de 150-250ms que a RFC pede.
const HIT_SHAKE_OSCILLATIONS: f32 = 3.0;

/// Golpe efetivo (`effective: true`) ou contra-ataque não-bloqueado —
/// reação "forte" (regras 2 e 3 da RFC).
const HIT_STRONG_DURATION_S: f32 = 0.22;
/// Bem acima do idle bob (4-5px, ver `hero_bob`/`foe_bob`) pra ser
/// distinguível dele, mas pequeno frente aos ~120-170px do retrato — não
/// pode competir com a leitura do editor (regra 6, inviolável desde a
/// RFC-001).
const HIT_STRONG_AMPLITUDE_PX: f32 = 10.0;
/// Visível mas o retrato continua legível — mitigação do risco "tingimento
/// deixa o retrato ilegível" que a própria RFC-020 lista.
const HIT_STRONG_TINT_ALPHA: f32 = 0.35;

/// Golpe de raspão (`effective: false`) ou contra-ataque bloqueado —
/// reação "fraca": no piso de duração da RFC e com amplitude na mesma
/// faixa do idle bob. "Quase não reagiu" É o feedback pretendido (regra 2:
/// um golpe de raspão com reação forte mentiria sobre o que aconteceu).
const HIT_WEAK_DURATION_S: f32 = 0.15;
const HIT_WEAK_AMPLITUDE_PX: f32 = 4.0;
const HIT_WEAK_TINT_ALPHA: f32 = 0.12;

/// Mais curto que a reação de dano — o lunge é só "o peso do golpe", não
/// deve prolongar-se e brigar visualmente com a reação de quem recebe.
const LUNGE_DURATION_S: f32 = 0.18;
/// Maior que o deslocamento de dano forte (10px) de propósito: avançar é
/// um movimento de corpo inteiro, o recuo é só o abalo do impacto.
const LUNGE_AMPLITUDE_PX: f32 = 14.0;

/// Recuo rápido + tingimento `SANGUE` decaindo a zero. Nasce em
/// `Phase::Executing` quando `Attacked`/`BonusStrike`/`CounterAttack` é
/// revelado (mesmo ponto onde `HitPopup` já nasce hoje) e expira sozinho.
#[derive(Clone, Copy)]
struct HitReaction {
    timer: f32,
    duration: f32,
    amplitude_px: f32,
    tint_alpha: f32,
}

impl HitReaction {
    /// `strong` decide o par de tabelas (forte = golpe efetivo/contra-ataque
    /// não bloqueado; fraco = raspão/bloqueado) — regras 2 e 3 da RFC-020.
    fn new(strong: bool) -> Self {
        if strong {
            HitReaction { timer: 0.0, duration: HIT_STRONG_DURATION_S, amplitude_px: HIT_STRONG_AMPLITUDE_PX, tint_alpha: HIT_STRONG_TINT_ALPHA }
        } else {
            HitReaction { timer: 0.0, duration: HIT_WEAK_DURATION_S, amplitude_px: HIT_WEAK_AMPLITUDE_PX, tint_alpha: HIT_WEAK_TINT_ALPHA }
        }
    }

    /// Deslocamento horizontal do instante atual: amplitude * envelope de
    /// decaimento linear * seno (o "shake" vai-volta-vai). Curva linear no
    /// decaimento é aceitável pra uma primeira versão (a própria RFC
    /// permite).
    fn shake_px(&self) -> f32 {
        let progress = (self.timer / self.duration).clamp(0.0, 1.0);
        let decay = 1.0 - progress;
        self.amplitude_px * decay * (progress * std::f32::consts::PI * HIT_SHAKE_OSCILLATIONS).sin()
    }

    /// Opacidade do tingimento no instante atual — decaimento linear a
    /// zero, mesmo espírito do `HitPopup` acima.
    fn tint_alpha_now(&self) -> f32 {
        let progress = (self.timer / self.duration).clamp(0.0, 1.0);
        self.tint_alpha * (1.0 - progress)
    }
}

/// Avanço/lunge de quem desfere o golpe, na direção do alvo. Nasce junto
/// com o `HitReaction` do lado oposto (mesmo evento, mesmo tick de
/// `Phase::Executing`), regra 4 da RFC-020.
#[derive(Clone, Copy)]
struct LungeReaction {
    timer: f32,
    duration: f32,
}

impl LungeReaction {
    fn new() -> Self {
        LungeReaction { timer: 0.0, duration: LUNGE_DURATION_S }
    }

    /// `direction`: +1.0 avança pra direita (jogador, que ataca o monstro à
    /// direita), -1.0 avança pra esquerda (monstro, que ataca o jogador à
    /// esquerda). Único hump de seno (0 -> pico -> 0): avança e volta, sem
    /// oscilar feito o shake do `HitReaction` — é um movimento, não um
    /// tremor.
    fn offset_px(&self, direction: f32) -> f32 {
        let progress = (self.timer / self.duration).clamp(0.0, 1.0);
        direction * LUNGE_AMPLITUDE_PX * (progress * std::f32::consts::PI).sin()
    }
}

/// Estado de animação reativa de um retrato inteiro (regra 5 da RFC-020) —
/// um valor destes por jogador, outro por monstro. `hit` e `lunge` são
/// independentes porque nascem de papéis diferentes no mesmo evento (quem
/// bateu vs. quem apanhou), e um retrato pode acumular os dois no mesmo
/// turno (ex.: o monstro ataca de volta: o jogador toma `hit`, o monstro
/// tem `lunge` — nunca os dois ao mesmo tempo no mesmo retrato, mas o tipo
/// não precisa saber disso pra funcionar).
#[derive(Clone, Copy, Default)]
struct PortraitAnim {
    hit: Option<HitReaction>,
    lunge: Option<LungeReaction>,
}

impl PortraitAnim {
    /// Chamado todo frame, independente da fase — mesmo padrão do timer de
    /// `HitPopup` em `update()`: a animação decai até o fim mesmo que a
    /// cena já tenha voltado pra `Phase::Writing` no meio do caminho.
    fn advance(&mut self, dt: f32) {
        if let Some(h) = &mut self.hit {
            h.timer += dt;
            if h.timer >= h.duration {
                self.hit = None;
            }
        }
        if let Some(l) = &mut self.lunge {
            l.timer += dt;
            if l.timer >= l.duration {
                self.lunge = None;
            }
        }
    }
}

/// Aplica a reação correspondente aos retratos quando um `TurnEvent` de
/// combate é revelado em `Phase::Executing` — mesmo ponto onde
/// `popup_for_event` já dispara o `HitPopup` hoje (regra 5 da RFC-020).
/// Eventos sem golpe (`Defended`, `Inspected`, `Healed`, `Waited`,
/// `Truncated`, `Selected`) não tocam nenhum dos dois retratos.
fn trigger_portrait_reactions(hero: &mut PortraitAnim, foe: &mut PortraitAnim, ev: &TurnEvent) {
    match ev {
        // regra 1 + 2: quem apanha é o monstro; a intensidade reflete
        // `effective`. Regra 4: o jogador (quem golpeou) avança.
        TurnEvent::Attacked { effective, .. } => {
            foe.hit = Some(HitReaction::new(*effective));
            hero.lunge = Some(LungeReaction::new());
        }
        // Golpe bônus (script eficiente) é sempre um acerto limpo — sem
        // campo `effective` porque a VM só o emite quando sobrou ciclo, ou
        // seja, nunca é "de raspão" (ver `script/vm.rs`); trata como forte.
        TurnEvent::BonusStrike { .. } => {
            foe.hit = Some(HitReaction::new(true));
            hero.lunge = Some(LungeReaction::new());
        }
        // regra 1 + 3: quem apanha é o jogador; a intensidade reflete o
        // inverso de `blocked` (bloqueado = reação mais suave). Regra 4: o
        // monstro (quem golpeou) avança.
        TurnEvent::CounterAttack { blocked, .. } => {
            hero.hit = Some(HitReaction::new(!*blocked));
            foe.lunge = Some(LungeReaction::new());
        }
        _ => {}
    }
}

/// RFC-009 regra 3: estado por cartão de comando — hover atualizado todo
/// frame e um flash de clique com timer próprio, no mesmo padrão do
/// `HitPopup` acima (nasce no evento discreto, expira sozinho).
#[derive(Clone, Copy, Default)]
struct CommandCardState {
    hovered: bool,
    flash: Option<f32>,
}

const COMMAND_FLASH_SECONDS: f32 = 0.12;

struct CommandEntry {
    label: &'static str,
    snippet: &'static str,
    cost_label: &'static str,
    /// RFC-033 regra 3: frase curta (~60-80 caracteres) de como usar o
    /// comando -- mostrada no hover do cartão da paleta (`draw_command_palette`)
    /// e reaproveitada como texto de apoio do autocomplete quando esta
    /// entrada está destacada (`draw_autocomplete_panel`).
    description: &'static str,
}

// Argumentos de item usam acesso "por enum" (`magia.Fogo`), sem aspas —
// equivalente a `magia["fogo"]`, mas lido como um enum em vez de string
// solta (ver script::vm::eval, caso Expr::Field sobre Value::Collection).
const COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        label: "atacar(item)",
        snippet: "atacar(espada.Fogo)",
        cost_label: "2c",
        description: "Ataca o inimigo com o item indicado -- dano cheio so contra a fraqueza dele.",
    },
    CommandEntry {
        label: "defender(item)",
        snippet: "defender(escudo.Bronze)",
        cost_label: "1c",
        description: "Reduz a metade o proximo contra-ataque, usando o item de defesa indicado.",
    },
    CommandEntry {
        label: "inspecionar()",
        snippet: "inspecionar()",
        cost_label: "3c",
        description: "Revela a fraqueza escondida do inimigo pelo resto do duelo.",
    },
    CommandEntry {
        label: "curar(item)",
        snippet: "curar(pocao.Vida)",
        cost_label: "4c",
        description: "Recupera vida do jogador usando o item de cura indicado.",
    },
    CommandEntry {
        label: "esperar()",
        snippet: "esperar()",
        cost_label: "1c",
        description: "Passa o turno sem agir -- gasta 1 ciclo, nao causa nem sofre dano.",
    },
    CommandEntry {
        label: "if cond:",
        snippet: "if inimigo.postura == \"guarda\":\n    ",
        cost_label: "1c",
        description: "Executa o bloco abaixo so quando a condicao for verdadeira.",
    },
    CommandEntry {
        label: "while cond:",
        snippet: "while inimigo.vida > 0:\n    ",
        cost_label: "1c/it",
        description: "Repete o bloco enquanto a condicao for verdadeira -- 1 ciclo por checagem.",
    },
    CommandEntry {
        label: "for i in a..b:",
        snippet: "for i in 0..3:\n    ",
        cost_label: "1c/it",
        description: "Repete o bloco para cada valor de i entre a (incluso) e b (excluso).",
    },
];

/// RFC-033 regra 1: palavras-chave do lexer (`script/lexer.rs::lex_ident`)
/// que não têm nenhum comando equivalente em `COMMANDS` -- `if`/`while`/`for`
/// já entram na lista de candidatos via `command_identifier` (a primeira
/// palavra do `label`), então não aparecem aqui de novo. Ordem fixa (mesma
/// do `match` do lexer) porque a RFC proíbe ranking por uso -- só
/// filtro por prefixo, ordem estável.
const EXTRA_KEYWORDS: &[&str] = &["else", "in", "func", "invocar", "selecionar", "and", "e", "or", "ou", "not", "nao", "true", "verdadeiro", "false", "falso"];

/// RFC-033 regra 1: um candidato do autocomplete -- `description` só existe
/// para comandos nativos (vem de `CommandEntry::description`); palavra-chave
/// e nome de `func` do jogador não têm descrição pronta.
struct AutocompleteCandidate {
    name: String,
    description: Option<&'static str>,
}

/// RFC-033 regra 1/2: lista de sugestões ancorada no cursor -- `row`/
/// `start_col` são a âncora (onde `identifier_prefix_before_cursor` disse
/// que o prefixo começa) e é o que `replace_identifier_prefix` usa para
/// completar sem tocar no resto da linha. `selected` é o índice destacado
/// (setas/clique trocam, nunca o filtro).
struct AutocompleteState {
    row: usize,
    start_col: usize,
    candidates: Vec<AutocompleteCandidate>,
    selected: usize,
}

/// Extrai a parte do `label` de um `CommandEntry` que é de fato o nome do
/// comando/palavra-chave -- a sequência de caracteres de identificador no
/// início (`atacar` de "atacar(item)", `if` de "if cond:"). É esse nome,
/// não o `label` inteiro, que entra na lista de candidatos do autocomplete.
fn command_identifier(label: &str) -> &str {
    let end = label.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(label.len());
    &label[..end]
}

pub struct DuelScene {
    editor: CodeEditor,
    phase: Phase,
    log: Vec<(String, Color)>,
    turn: u32,
    hit: Option<HitPopup>,
    /// RFC-020, regra 5: estado de animação reativa por retrato — um para
    /// o jogador, outro para o monstro.
    hero_anim: PortraitAnim,
    foe_anim: PortraitAnim,
    btn_execute: Button,
    /// RFC-027: ao lado de EXECUTAR — simula o duelo inteiro sobre um
    /// clone, sem gastar turno real. Ver `run_rehearsal`.
    btn_rehearse: Button,
    btn_leave: Button,
    btn_clear: Button,
    /// RFC-002, regra 10: grava o texto atual do editor em
    /// `SaveData::scripts`. Fica ao lado de `btn_clear` no mesmo padrão
    /// visual (botão pequeno no topo do editor).
    btn_save_script: Button,
    /// RFC-026 regra 2: abre a lista de `save.scripts` (mesmo cartão da
    /// paleta de comandos) para recarregar um script salvo no editor. Fica
    /// ao lado de `btn_save_script`, no mesmo estilo Ghost.
    btn_load_script: Button,
    /// `true` enquanto a lista de scripts salvos está sobreposta à tela —
    /// bloqueia edição/execução/paleta de comandos igual a `Phase::Executing`
    /// já bloqueia Executar, sem precisar virar uma variante de `Phase`
    /// (não é uma fase do turno, é um painel por cima da fase atual).
    show_load_menu: bool,
    command_cards: Vec<CommandCardState>,
    /// Variáveis do jogador que sobrevivem entre turnos do mesmo duelo
    /// (RFC-010). Vazio ao entrar no duelo e descartado junto com a cena
    /// ao sair dele — é assim que "nunca entre duelos diferentes"
    /// (não-objetivo 1 da RFC) é cumprido sem lógica de limpeza explícita.
    player_vars: HashMap<String, Value>,
    /// RFC-018: última validação ao vivo do texto do editor. Ver `LiveCheck`.
    live_check: LiveCheck,
    /// RFC-027: último resultado do Ensaio Geral, mostrado em
    /// `draw_rehearsal_overlay` enquanto `show_rehearsal` é `true`. `None`
    /// antes do primeiro ENSAIAR da cena — nunca inicializado com um
    /// resultado inventado.
    rehearsal: Option<rehearsal::RehearsalReport>,
    /// `true` enquanto o painel do Ensaio está sobreposto à tela — mesmo
    /// padrão modal de `show_load_menu` (bloqueia o resto de `update()`
    /// naquele frame).
    show_rehearsal: bool,
    /// RFC-029: `cycles_used` de cada turno realmente executado (só o
    /// braço `Ok(r)` de `run_script` empurra aqui — um turno que nem
    /// chegou a parsear nunca conta). É o histórico que a Grade de
    /// Eficiência lê para calcular turnos gastos e ciclos médios por turno
    /// do duelo inteiro (`turns_played`/`total_cycles_used` abaixo) — sem
    /// isso, `DuelScene` só sabia o resultado do *último* turno
    /// (`Phase::Executing`), nunca o acumulado do duelo.
    cycles_history: Vec<u32>,
    /// RFC-030 regra 2: nomes de `func` definidos pelo último script rodado
    /// (`run_script`), lidos antes de `self.editor.clear()` apagar o texto
    /// -- é o que a checagem de `DuelOutcome::Won` consulta pra saber se há
    /// algo a oferecer na tela de escolha. Sobrescrito a cada turno, vazio
    /// se o script não define nenhuma `func`.
    last_run_funcs: Vec<String>,
    /// `true` enquanto a tela de escolha pós-vitória está sobreposta —
    /// mesmo padrão modal de `show_load_menu`/`show_rehearsal` (bloqueia o
    /// resto de `update()` naquele frame). Só fica `true` quando
    /// `last_run_funcs` não estava vazio no momento da vitória (regra 2 da
    /// RFC-030: sem `func` no script vencedor, pula direto pro destino
    /// normal sem esta tela).
    show_compile_choice: bool,
    /// Nomes oferecidos na tela de escolha atual — cópia de
    /// `last_run_funcs` tirada no instante da vitória (regra 5: no máximo 1
    /// destes vira `save.compiled_funcs` por vitória de fase, e a tela some
    /// depois da primeira escolha ou de "pular").
    compile_choice_names: Vec<String>,
    /// QA (BAIXO-1, auditoria de interação): `true` só quando este duelo é
    /// o 7º e último da campanha (`save.current_phase + 1 >= PHASES.len()`,
    /// decidido por `PhaseScene::new` — `DuelScene` em si não conhece
    /// `PHASES`). Vencer com uma `func` no script pula a tela de escolha de
    /// RFC-030 quando `true`: não há duelo futuro nenhum onde a isenção de
    /// `USER_CALL_COST` teria efeito, então a escolha não tem consequência
    /// — só atrapalharia o clímax da vitória final. `OverworldScene` (mapa
    /// de debug, sem noção de campanha linear) sempre passa `false`.
    is_final_phase: bool,
    /// RFC-033 regra 1: lista de sugestões ativa neste frame, ou `None` se
    /// o cursor não está num prefixo de identificador ou nenhum candidato
    /// combina. Recalculada em `recompute_autocomplete`, só durante
    /// `Phase::Writing`.
    autocomplete: Option<AutocompleteState>,
    /// RFC-033 regra 2: retângulo de cada linha do painel de autocomplete
    /// desenhado no frame anterior -- `draw_autocomplete_panel` (`&self`)
    /// escreve aqui, `update` (`&mut self`, roda antes do próximo `draw`) lê
    /// para decidir se um clique acertou alguma sugestão. Existe só por essa
    /// ordem update-antes-de-draw: sem isto, `update` precisaria da mesma
    /// medição de `font_code` que hoje só `draw` recebe (`Assets`), o que
    /// obrigaria a passar `Assets` por `PhaseScene`/`OverworldScene`/
    /// `main.rs` só para isto -- um frame de atraso no hit-test do clique é
    /// imperceptível e evita esse refactor fora do escopo desta RFC.
    autocomplete_rects: RefCell<Vec<Rect>>,
}

impl DuelScene {
    pub fn new(is_final_phase: bool) -> Self {
        DuelScene {
            is_final_phase,
            editor: CodeEditor::new(),
            phase: Phase::Writing,
            log: vec![("Escreva um script e aperte EXECUTAR (ou F5).".to_string(), theme::POEIRA)],
            turn: 1,
            hit: None,
            hero_anim: PortraitAnim::default(),
            foe_anim: PortraitAnim::default(),
            // RFC-027 regra 1: EXECUTAR/ENSAIAR/FUGIR dividem a mesma linha
            // (440px uteis = EDITOR_W - 20). FUGIR e ENSAIAR ficam com
            // largura fixa (90/96), EXECUTAR (ação principal) fica com o
            // resto — mesmo raciocínio de largura fixa que já existia entre
            // EXECUTAR e FUGIR antes desta RFC.
            btn_execute: Button::new("EXECUTAR", vec2(10.0, BUTTONS_Y), vec2(EDITOR_W - 20.0 - 90.0 - 96.0 - 16.0, 56.0), ButtonStyle::Primary, theme::TITLE_SM),
            btn_rehearse: Button::new("ENSAIAR", vec2(10.0 + (EDITOR_W - 20.0 - 90.0 - 96.0 - 16.0) + 8.0, BUTTONS_Y), vec2(96.0, 56.0), ButtonStyle::Secondary, theme::TITLE_SM),
            btn_leave: Button::new("FUGIR", vec2(10.0 + EDITOR_W - 20.0 - 90.0, BUTTONS_Y), vec2(90.0, 56.0), ButtonStyle::Secondary, theme::TITLE_SM),
            // RFC-032 regra 1: os 3 botões mudaram de EDITOR_BOX_Y + 5.0 (1a
            // linha, disputando X com "{N} LINHAS") para
            // EDITOR_HEADER_BUTTONS_Y (2a linha da barra) — X de cada botão
            // é o mesmo de antes (só o Y mudou), o gap de 8px entre os três
            // já estava correto e continua valendo.
            btn_clear: Button::new("LIMPAR", vec2(EDITOR_W - 90.0, EDITOR_HEADER_BUTTONS_Y), vec2(78.0, 26.0), ButtonStyle::Ghost, 12),
            btn_save_script: Button::new("SALVAR", vec2(EDITOR_W - 90.0 - 86.0, EDITOR_HEADER_BUTTONS_Y), vec2(78.0, 26.0), ButtonStyle::Ghost, 12),
            // "CARREGAR" tem 2 letras a mais que "SALVAR"/"LIMPAR" — caixa
            // um pouco mais larga (84 em vez de 78) pro rótulo não estourar
            // a borda a 12px, mesmo gap de 8px que já separa os outros dois.
            btn_load_script: Button::new("CARREGAR", vec2(EDITOR_W - 90.0 - 86.0 - 92.0, EDITOR_HEADER_BUTTONS_Y), vec2(84.0, 26.0), ButtonStyle::Ghost, 12),
            show_load_menu: false,
            command_cards: vec![CommandCardState::default(); COMMANDS.len()],
            player_vars: HashMap::new(),
            // editor começa vazio; `Valid { cycles_used: 0, .. }` é o
            // resultado real de validar um script vazio (nenhuma chamada,
            // nenhum ciclo) — não é um placeholder otimista.
            live_check: LiveCheck::Valid { cycles_used: 0, truncated: false },
            rehearsal: None,
            show_rehearsal: false,
            cycles_history: Vec::new(),
            last_run_funcs: Vec::new(),
            show_compile_choice: false,
            compile_choice_names: Vec::new(),
            autocomplete: None,
            autocomplete_rects: RefCell::new(Vec::new()),
        }
    }

    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// RFC-029: quantos turnos realmente executaram um script válido neste
    /// duelo (`cycles_history.len()`) — diferente de `self.turn()`, que já
    /// incrementa mesmo num turno que falhou o parse (ver `run_script`).
    /// É este número, não `turn()`, que a Grade de Eficiência usa como
    /// "turnos gastos".
    pub fn turns_played(&self) -> u32 {
        self.cycles_history.len() as u32
    }

    /// RFC-029: soma de `cycles_used` de todos os turnos executados no
    /// duelo inteiro — o dado bruto que `grade::apply_duel_result` usa
    /// para calcular ciclos médios por turno, sem nenhum cálculo novo de
    /// VM (regra 1 da RFC).
    pub fn total_cycles_used(&self) -> u32 {
        self.cycles_history.iter().sum()
    }

    fn command_rect(index: usize) -> Rect {
        let col = (index % 2) as f32;
        let row = (index / 2) as f32;
        let w = (EDITOR_W - 20.0 - 8.0) / 2.0;
        Rect::new(10.0 + col * (w + 8.0), COMMAND_PANEL_Y + 26.0 + row * (COMMAND_ROW_H + COMMAND_ROW_GAP), w, COMMAND_ROW_H)
    }

    /// RFC-033 regra 1: tenta parsear o texto atual do editor e devolve os
    /// nomes de `func` já escritos nele (`vm::defined_func_names`, mesma
    /// função que RFC-030 usa para a tela de escolha pós-vitória). Um script
    /// com erro de sintaxe no meio da digitação (o caso comum) simplesmente
    /// não contribui nomes de func nesse frame -- não é um erro do
    /// autocomplete, é a mesma limitação que já existe para qualquer análise
    /// que dependa de um parse bem-sucedido (ver `compute_live_check` acima).
    fn current_func_names(src: &str) -> Vec<String> {
        match parser::parse(src) {
            Ok(program) => vm::defined_func_names(&program),
            Err(_) => Vec::new(),
        }
    }

    /// RFC-033 regra 1: lista completa (não filtrada) de candidatos do
    /// autocomplete, nesta ordem estável: nomes de `COMMANDS` (ordem do
    /// array), palavras-chave que faltam (`EXTRA_KEYWORDS`, ordem fixa),
    /// nomes de `func` do script atual (ordem de definição). Duplicata por
    /// nome é descartada mantendo a primeira ocorrência -- só pode
    /// acontecer se o jogador nomear uma `func` igual a um comando/palavra-
    /// chave, caso em que a descrição do comando nativo prevalece.
    fn all_autocomplete_candidates(src: &str) -> Vec<AutocompleteCandidate> {
        let mut out: Vec<AutocompleteCandidate> = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        for cmd in COMMANDS {
            let name = command_identifier(cmd.label).to_string();
            let key = name.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            out.push(AutocompleteCandidate { name, description: Some(cmd.description) });
        }
        for kw in EXTRA_KEYWORDS {
            let key = kw.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            out.push(AutocompleteCandidate { name: kw.to_string(), description: None });
        }
        for name in Self::current_func_names(src) {
            let key = name.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            out.push(AutocompleteCandidate { name, description: None });
        }
        out
    }

    /// RFC-033 regra 1: recalculada a cada frame de `Phase::Writing` em que
    /// o autocomplete não foi explicitamente fechado/consumido por uma tecla
    /// (Esc/Tab/Enter) ou navegado (setas) -- ver o bloco de `update()`.
    /// Fecha a lista (`None`) quando o cursor não está num prefixo de
    /// identificador (regra 1) ou quando nenhum candidato combina com o
    /// prefixo (critério de aceite: "digitar algo que não combina com nada
    /// não mostra lista nenhuma"). Preserva o `selected` atual quando a
    /// âncora (linha/coluna de início) não mudou -- evita que o destaque
    /// pule de volta pro topo a cada tecla digitada dentro do mesmo prefixo.
    fn recompute_autocomplete(&mut self) {
        let Some((start_col, prefix)) = self.editor.identifier_prefix_before_cursor() else {
            self.autocomplete = None;
            return;
        };
        let prefix_lower = prefix.to_lowercase();
        let src = self.editor.text();
        let candidates: Vec<AutocompleteCandidate> = Self::all_autocomplete_candidates(&src)
            .into_iter()
            .filter(|c| c.name.to_lowercase().starts_with(&prefix_lower))
            .take(5)
            .collect();
        if candidates.is_empty() {
            self.autocomplete = None;
            return;
        }
        let row = self.editor.cursor_row;
        let selected = match &self.autocomplete {
            Some(prev) if prev.row == row && prev.start_col == start_col => prev.selected.min(candidates.len() - 1),
            _ => 0,
        };
        self.autocomplete = Some(AutocompleteState { row, start_col, candidates, selected });
    }

    /// RFC-033 regra 2: retângulo da linha `index` dentro do painel de
    /// autocomplete cuja origem é `origin` -- geometria pura, sem depender
    /// de fonte, para que `draw_autocomplete_panel` (que sabe a origem real,
    /// calculada com `font_code`) e o cache de clique em `update` concordem
    /// sobre onde cada sugestão está.
    fn autocomplete_row_rect(origin: Vec2, index: usize, width: f32) -> Rect {
        const ROW_H: f32 = 22.0;
        Rect::new(origin.x, origin.y + index as f32 * ROW_H, width, ROW_H)
    }

    /// RFC-026 regra 2: retângulo do cartão `index` na lista de scripts
    /// salvos — extraído aqui pra que clique (em `update`) e desenho (em
    /// `draw_load_overlay`) nunca divirjam, mesmo padrão de `command_rect`
    /// acima e de `slot_rect` em `grimoire.rs`.
    fn load_card_rect(index: usize) -> Rect {
        let x = WIDTH * 0.2;
        let w = WIDTH * 0.6;
        let y = 110.0 + index as f32 * 64.0;
        Rect::new(x, y, w, 56.0)
    }

    /// RFC-030 regra 2: retângulo do cartão `index` na tela de escolha
    /// pós-vitória -- mesmo raciocínio de `load_card_rect` acima (clique em
    /// `update` e desenho em `draw_compile_choice_overlay` nunca podem
    /// divergir).
    fn compile_choice_card_rect(index: usize) -> Rect {
        let x = WIDTH * 0.2;
        let w = WIDTH * 0.6;
        let y = 130.0 + index as f32 * 64.0;
        Rect::new(x, y, w, 56.0)
    }

    /// Cartão "PULAR" (nenhuma func compilada), sempre logo abaixo do
    /// último cartão de nome -- `names_len` desloca sua posição pra nunca
    /// sobrepor os cartões de nome acima dele.
    fn compile_choice_skip_rect(names_len: usize) -> Rect {
        let x = WIDTH * 0.2;
        let w = WIDTH * 0.6;
        let y = 130.0 + names_len as f32 * 64.0 + 16.0;
        Rect::new(x, y, w, 48.0)
    }

    /// QA (ALTO-2, auditoria de interação RFC-023/026-030): `true` enquanto
    /// qualquer overlay modal (CARREGAR/RFC-026, ENSAIAR/RFC-027, escolha de
    /// função/RFC-030) está aberto -- cada um documenta "ESC fecha/pula" como
    /// comportamento próprio, mas `main.rs` consumia esse mesmo `ESC` pra
    /// abrir o menu de pausa antes de `DuelScene::update()` rodar, então o
    /// overlay nunca via a tecla. `PhaseScene`/`main.rs` consultam isto pra
    /// decidir: com um overlay aberto, o `ESC` deste frame pertence à cena,
    /// não à pausa.
    pub fn has_modal_overlay_open(&self) -> bool {
        self.show_load_menu || self.show_rehearsal || self.show_compile_choice
    }

    pub fn update(&mut self, player: &mut Entity, monster: &mut MonsterState, save: &mut SaveData) -> Option<DuelOutcome> {
        let mouse: Vec2 = virtual_mouse_position().into();
        self.btn_execute.update_hover(mouse);
        self.btn_rehearse.update_hover(mouse);
        self.btn_leave.update_hover(mouse);
        self.btn_clear.update_hover(mouse);
        self.btn_save_script.update_hover(mouse);
        self.btn_load_script.update_hover(mouse);
        // regra 2: Executar fica desabilitado enquanto o turno está sendo
        // reproduzido — a proteção mora no próprio Button, não espalhada.
        self.btn_execute.disabled = matches!(self.phase, Phase::Executing { .. });
        // RFC-027 regra 5: Ensaiar não teria efeito visível durante o
        // replay do turno real (mesmo texto do editor que já foi
        // consumido) — desabilitado pelo mesmo motivo do Executar acima.
        self.btn_rehearse.disabled = matches!(self.phase, Phase::Executing { .. });
        // RFC-026 regra 2: abrir a lista não faz sentido no meio do replay
        // do turno (mesmo motivo de Executar acima).
        self.btn_load_script.disabled = matches!(self.phase, Phase::Executing { .. });

        if let Some(hit) = &mut self.hit {
            hit.timer += get_frame_time();
            if hit.timer > 1.1 {
                self.hit = None;
            }
        }

        // RFC-020: decai independente da fase, mesmo padrão do timer de
        // `hit` acima — a reação termina sozinha mesmo se o turno já
        // acabou (Phase::Executing -> Phase::Writing) no meio do caminho.
        let dt = get_frame_time();
        self.hero_anim.advance(dt);
        self.foe_anim.advance(dt);

        // regra 3: o flash de clique decai independente da fase corrente,
        // mesmo padrão do timer de `hit` acima.
        for card in self.command_cards.iter_mut() {
            if let Some(t) = &mut card.flash {
                *t += get_frame_time();
                if *t > COMMAND_FLASH_SECONDS {
                    card.flash = None;
                }
            }
        }

        // QA (achado de interação RFC-026/027/030): FUGIR/LIMPAR/SALVAR/
        // CARREGAR eram checados aqui incondicionalmente, antes dos blocos
        // modais abaixo -- um clique nas coordenadas desses botões
        // (visualmente cobertos pelo retângulo full-screen de qualquer um
        // dos três overlays) ainda disparava a ação real por baixo (fugir,
        // limpar o editor, salvar, ou abrir CARREGAR por cima de outro
        // overlay já aberto, deixando dois `show_*` simultaneamente `true`
        // e os dois desenhados empilhados em `draw()`). Nenhum dos três
        // overlays é de fato modal para clique, só visualmente. Guarda
        // única: enquanto qualquer um estiver aberto, nenhum desses quatro
        // botões reage -- cada overlay continua resolvendo seu próprio
        // fechamento/clique nos blocos abaixo, sem mudança de comportamento
        // para o caso comum (nenhum overlay aberto).
        if !self.has_modal_overlay_open() {
            if self.btn_leave.clicked(mouse) {
                return Some(DuelOutcome::Fled);
            }
            if self.btn_clear.clicked(mouse) {
                self.editor.clear();
                // RFC-033: LIMPAR muda o texto por baixo de qualquer lista
                // de sugestões aberta -- a âncora dela (linha/coluna) não
                // existe mais depois de um editor vazio.
                self.autocomplete = None;
            }
            if self.btn_save_script.clicked(mouse) {
                self.save_current_script(save);
            }
            if self.btn_load_script.clicked(mouse) {
                if save.scripts.is_empty() {
                    self.log.push(("Nenhum script salvo para carregar.".to_string(), theme::POEIRA));
                } else {
                    self.show_load_menu = !self.show_load_menu;
                }
            }
        }

        // RFC-026 regra 2: enquanto a lista está aberta, ela consome o
        // clique inteiro (carregar um script ou fechar a lista) e nada mais
        // do resto de `update()` de `Phase::Writing` roda neste frame —
        // mesmo raciocínio de "Executar desabilitado durante o replay":
        // a lista é uma sobreposição modal, não mais um painel entre outros.
        if self.show_load_menu {
            if is_key_pressed(KeyCode::Escape) {
                self.show_load_menu = false;
            } else if is_mouse_button_pressed(MouseButton::Left) {
                for (i, script) in save.scripts.iter().enumerate() {
                    if Self::load_card_rect(i).contains(mouse) {
                        self.editor.load_text(&script.body);
                        // RFC-033: mesmo raciocínio do LIMPAR acima -- texto
                        // trocado por baixo de uma âncora que já não existe.
                        self.autocomplete = None;
                        self.log.push((format!("Script carregado do grimorio: {}", script.name), theme::MUSGO));
                        self.show_load_menu = false;
                        break;
                    }
                }
            }
            return None;
        }

        // RFC-027 regra 5: o painel do Ensaio é uma sobreposição modal, mesmo
        // padrão de `show_load_menu` acima — enquanto está aberto, consome o
        // frame inteiro (só ESC ou clicar ENSAIAR de novo fecha).
        if self.show_rehearsal {
            if is_key_pressed(KeyCode::Escape) || self.btn_rehearse.clicked(mouse) {
                self.show_rehearsal = false;
            }
            return None;
        }

        // RFC-030 regra 2: a tela de escolha pós-vitória consome o frame
        // inteiro, mesmo padrão modal de `show_load_menu`/`show_rehearsal`
        // acima -- só sai daqui escolhendo uma func (clique num cartão) ou
        // pulando (ESC ou o cartão "PULAR"), e só então devolve o
        // `DuelOutcome::Won` que ficou pendente desde o fim do turno que
        // matou o monstro.
        if self.show_compile_choice {
            if is_key_pressed(KeyCode::Escape) {
                self.show_compile_choice = false;
                return Some(DuelOutcome::Won);
            }
            if is_mouse_button_pressed(MouseButton::Left) {
                for (i, name) in self.compile_choice_names.iter().enumerate() {
                    if Self::compile_choice_card_rect(i).contains(mouse) {
                        // regra 4: a isenção é por nome -- se este nome já
                        // estava compilado (ex.: o jogador redefiniu e
                        // venceu de novo escolhendo o mesmo nome), não
                        // duplica a entrada no save.
                        if !save.compiled_funcs.iter().any(|compiled| compiled == name) {
                            save.compiled_funcs.push(name.clone());
                            self.log.push((format!("Funcao '{name}' compilada -- chamadas futuras dela sao de graca."), theme::OURO));
                        }
                        self.show_compile_choice = false;
                        return Some(DuelOutcome::Won);
                    }
                }
                if Self::compile_choice_skip_rect(self.compile_choice_names.len()).contains(mouse) {
                    self.show_compile_choice = false;
                    return Some(DuelOutcome::Won);
                }
            }
            return None;
        }

        let writing = matches!(self.phase, Phase::Writing | Phase::Error(_));
        if writing {
            for (i, cmd) in COMMANDS.iter().enumerate() {
                let r = Self::command_rect(i);
                let card = &mut self.command_cards[i];
                card.hovered = r.contains(mouse);
                if card.hovered && is_mouse_button_pressed(MouseButton::Left) {
                    self.editor.insert_snippet(cmd.snippet);
                    card.flash = Some(0.0);
                    // RFC-033: um clique na paleta insere texto (possivelmente
                    // com quebra de linha) na posição do cursor -- a mesma
                    // razão do LIMPAR/CARREGAR acima, a âncora de qualquer
                    // lista de sugestões aberta não sobrevive a isso.
                    self.autocomplete = None;
                }
            }
        } else {
            for card in self.command_cards.iter_mut() {
                card.hovered = false;
            }
        }

        match &mut self.phase {
            Phase::Writing => {
                // RFC-033 regra 2: enquanto a lista de sugestões está
                // aberta, Esc/setas/Tab/Enter pertencem a ela primeiro --
                // nenhuma dessas teclas deve também ser interpretada por
                // `CodeEditor::update()` no mesmo frame (Tab, por exemplo,
                // indentaria 4 espaços por baixo da conclusão do
                // identificador se os dois rodassem). `consumed` marca que o
                // frame já foi tratado e `self.editor.update()` não deve
                // rodar; nesses casos a lista também não é recalculada
                // depois (deixa o fechamento/seleção/conclusão valendo,
                // em vez de reabrir/resetar no mesmo frame).
                let mut consumed = false;
                if let Some(ac) = &mut self.autocomplete {
                    if is_key_pressed(KeyCode::Escape) {
                        self.autocomplete = None;
                        consumed = true;
                    } else if is_key_pressed(KeyCode::Up) {
                        ac.selected = if ac.selected == 0 { ac.candidates.len() - 1 } else { ac.selected - 1 };
                        consumed = true;
                    } else if is_key_pressed(KeyCode::Down) {
                        ac.selected = (ac.selected + 1) % ac.candidates.len();
                        consumed = true;
                    } else if is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::Enter) {
                        let name = ac.candidates[ac.selected].name.clone();
                        let start_col = ac.start_col;
                        self.editor.replace_identifier_prefix(start_col, &name);
                        self.autocomplete = None;
                        consumed = true;
                    } else if is_mouse_button_pressed(MouseButton::Left) {
                        // Ajuste do product-manager sobre a ambiguidade
                        // sinalizada na entrega: clicar numa sugestão
                        // confirma na hora, mesmo padrão universal de
                        // autocomplete (editor de código, busca de
                        // navegador, etc.) -- exigir um Tab/Enter extra
                        // depois do clique seria surpreendente pra quem já
                        // usou qualquer autocomplete. As setas continuam só
                        // destacando (não confirmam sozinhas), porque
                        // navegação por teclado sem confirmação explícita
                        // é o padrão esperado nesse caso. Usa o retângulo
                        // cacheado do último `draw` (ver `autocomplete_rects`,
                        // campo documentado em `DuelScene`).
                        for (i, r) in self.autocomplete_rects.borrow().iter().enumerate() {
                            if r.contains(mouse) && i < ac.candidates.len() {
                                let name = ac.candidates[i].name.clone();
                                let start_col = ac.start_col;
                                self.editor.replace_identifier_prefix(start_col, &name);
                                self.autocomplete = None;
                                consumed = true;
                                break;
                            }
                        }
                    }
                }

                if !consumed {
                    self.editor.update();
                    self.recompute_autocomplete();
                }
                // RFC-018 regra 1: recalculado a cada frame a partir do
                // texto atual — nunca uma heurística, nunca um valor
                // desatualizado da última edição.
                self.live_check = Self::compute_live_check(&self.editor.text(), player, monster, save, &self.player_vars);
                // RFC-027 regra 1: atalho `Shift+F5` para ENSAIAR — `F5`
                // sozinho continua sendo EXECUTAR (regra 1 confirma isso
                // olhando o `is_key_pressed(KeyCode::F5)` logo abaixo, já
                // existente antes desta RFC). `shift` decide qual dos dois
                // dispara, nunca os dois no mesmo frame.
                let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
                let want_rehearse = self.btn_rehearse.clicked(mouse) || (shift && is_key_pressed(KeyCode::F5));
                let want_run = self.btn_execute.clicked(mouse) || (is_key_pressed(KeyCode::F5) && !shift);
                if want_rehearse {
                    self.run_rehearsal(player, monster, save);
                } else if want_run {
                    self.run_script(player, monster, save);
                }
            }
            Phase::Executing { result, index, timer } => {
                *timer += get_frame_time();
                if *timer >= EVENT_TICK_SECONDS {
                    *timer = 0.0;
                    if *index < result.events.len() {
                        let ev = &result.events[*index];
                        self.log.push(describe_event(ev));
                        self.editor.highlighted_line = event_line(ev);
                        if let Some(popup) = popup_for_event(ev) {
                            self.hit = Some(popup);
                        }
                        // RFC-020, regra 5: mesmo ponto onde o HitPopup
                        // nasce — a reação de retrato usa o mesmo gatilho.
                        trigger_portrait_reactions(&mut self.hero_anim, &mut self.foe_anim, ev);
                        *index += 1;
                    } else {
                        self.editor.clear();
                        self.phase = Phase::Writing;
                        // RFC-026 regra 1: o fim de duelo só é informado à
                        // `PhaseScene` aqui, depois que o replay inteiro do
                        // turno (log/retrato/dano, um evento por tick acima)
                        // já tocou — antes, essa checagem rodava a cada
                        // `update()` incondicional à fase e cortava o
                        // replay do turno que matava o monstro, porque
                        // `run_script` já tinha aplicado o resultado da VM
                        // de forma síncrona antes de qualquer evento ser
                        // revelado. Nenhuma regra de combate muda — só o
                        // momento em que a cena comunica o resultado.
                        if !monster.alive() {
                            // RFC-030 regra 2: só oferece a tela de escolha
                            // se o script que venceu definiu pelo menos uma
                            // `func` -- `last_run_funcs` foi gravado em
                            // `run_script`, antes de `self.editor.clear()`
                            // apagar o texto do turno vencedor. Sem
                            // nenhuma func, pula direto pro `Won` normal,
                            // sem forcar nada (não-objetivo 2 da RFC).
                            if !self.last_run_funcs.is_empty() && !self.is_final_phase {
                                self.compile_choice_names = std::mem::take(&mut self.last_run_funcs);
                                self.show_compile_choice = true;
                                return None;
                            }
                            return Some(DuelOutcome::Won);
                        }
                        if player.life_points <= 0 {
                            player.alive = false;
                            return Some(DuelOutcome::Lost);
                        }
                    }
                }
            }
            Phase::Error(_) => {
                self.editor.update();
                if is_key_pressed(KeyCode::Enter) || self.btn_execute.clicked(mouse) {
                    self.phase = Phase::Writing;
                }
            }
        }

        None
    }

    /// RFC-002, regra 10: grava o conteúdo atual do editor
    /// (`CodeEditor::text()`) como um novo `SavedScript` no `SaveData` da
    /// expedição. Persiste no disco imediatamente (não só ao sair do
    /// overworld/duelo, como o resto do save) porque um clique explícito
    /// de "salvar" é a única ação desta RFC que o jogador pode esperar
    /// sobreviver mesmo se ele fugir do duelo em seguida (fuga não é um
    /// dos gatilhos de persistência do overworld) — perder um script que
    /// o próprio jogador mandou salvar seria pior que o custo de um write
    /// extra fora do loop de frame (é um clique, não algo por frame).
    /// Script vazio não gera entrada: não há nada útil pra nomear/carregar
    /// depois.
    fn save_current_script(&mut self, save: &mut SaveData) {
        let body = self.editor.text();
        if body.trim().is_empty() {
            self.log.push(("Nada para salvar: o editor esta vazio.".to_string(), theme::POEIRA));
            return;
        }
        let name = format!("script-{}.pii", save.scripts.len() + 1);
        self.log.push((format!("Script salvo no grimorio: {name}"), theme::MUSGO));
        save.scripts.push(SavedScript { name, body });
        save.save();
    }

    fn run_script(&mut self, player: &mut Entity, monster: &mut MonsterState, save: &SaveData) {
        // RFC-033: EXECUTAR sai de `Phase::Writing` (e o texto do editor
        // pode ser apagado mais adiante, quando o replay termina) -- uma
        // lista de sugestões apontando para uma linha/coluna que já não
        // existe mais não deve sobreviver a essa transição.
        self.autocomplete = None;
        self.turn += 1;
        let src = self.editor.text();
        let program = match parser::parse(&src) {
            Ok(p) => p,
            Err(e) => {
                self.log.push((format!("Erro: {e}"), theme::SANGUE));
                self.phase = Phase::Error(e);
                return;
            }
        };

        // RFC-030 regra 2: guarda os nomes de `func` deste script antes de
        // `self.editor.clear()` (mais abaixo, quando os eventos do turno
        // terminam de tocar) apagar o texto -- é o que a checagem de
        // `DuelOutcome::Won` consulta pra decidir se mostra a tela de
        // escolha.
        self.last_run_funcs = vm::defined_func_names(&program);

        // RFC-027: `vm::simulate_turn_with_compiled_funcs` é a mesma rotina
        // que `script::rehearsal::rehearse_with_compiled_funcs` chama sobre
        // um clone — extraída pra que o turno real e o Ensaio nunca
        // divirjam na progressão de carga/postura/consumo de carga
        // especial. RFC-030: passa `save.compiled_funcs` pra que uma func
        // já compilada não cobre `USER_CALL_COST` de novo neste turno.
        let result = vm::simulate_turn_with_compiled_funcs(
            &program,
            &mut self.player_vars,
            monster,
            player.life_points,
            player.max_life,
            Some(&save.loadout),
            save.player_class,
            Some(&save.bag),
            &save.compiled_funcs,
        );

        match result {
            Ok(r) => {
                player.life_points = r.player_life;
                // RFC-029: acumula antes de mover `r` para `Phase::Executing`
                // -- é este histórico que sobrevive ao duelo inteiro, ao
                // contrário de `Phase::Executing`, que só guarda o último
                // turno.
                self.cycles_history.push(r.cycles_used);
                self.phase = Phase::Executing { result: r, index: 0, timer: EVENT_TICK_SECONDS };
            }
            Err(e) => {
                self.log.push((format!("Erro: {e}"), theme::SANGUE));
                self.phase = Phase::Error(e);
            }
        }
    }

    /// ENSAIAR (RFC-027): nunca recebe `&mut` de `player`/`monster` —
    /// prova, em nível de tipo, que esta função é incapaz de tocar o
    /// estado real, além do teste que compara bit-a-bit em
    /// `script::rehearsal`. Erro de sintaxe/tipo mostra o mesmo erro que
    /// EXECUTAR mostraria (regra 2) e não avança `self.turn` nem chama
    /// `rehearse` — ENSAIAR nunca custa um turno real (regra 5).
    fn run_rehearsal(&mut self, player: &Entity, monster: &MonsterState, save: &SaveData) {
        // RFC-033: abre o overlay modal do Ensaio (`show_rehearsal`), que
        // consome o resto de `update()` até fechar -- mesmo raciocínio de
        // `run_script` acima, sem deixar a lista velha reaparecendo por
        // baixo do overlay quando ele fechar.
        self.autocomplete = None;
        let src = self.editor.text();
        let program = match parser::parse(&src) {
            Ok(p) => p,
            Err(e) => {
                self.log.push((format!("Erro: {e}"), theme::SANGUE));
                self.phase = Phase::Error(e);
                return;
            }
        };
        let report = rehearsal::rehearse_with_compiled_funcs(
            &program,
            &self.player_vars,
            player.life_points,
            player.max_life,
            monster,
            Some(&save.loadout),
            save.player_class,
            Some(&save.bag),
            &save.compiled_funcs,
        );
        self.rehearsal = Some(report);
        self.show_rehearsal = true;
    }

    /// RFC-018: validação ao vivo do texto atual do editor — parser real
    /// mais uma passada de validação real da VM (`vm::probe_turn_with_bag`,
    /// só a metade "dry-run" de `run_turn_with_bag`, sem a passada real que
    /// aplicaria efeito). Não mexe em `monster` (nada de `begin_turn`/carga
    /// — isso só acontece no turno de verdade, em `run_script`) e nunca
    /// passa o `player_vars` original para a VM: `probe_turn_with_bag` só
    /// lê `&self.player_vars` e clona internamente antes de rodar, então o
    /// estado real do jogador não pode vazar aqui não importa quantas vezes
    /// isto rode por segundo sem o jogador apertar EXECUTAR.
    fn compute_live_check(src: &str, player: &Entity, monster: &MonsterState, save: &SaveData, player_vars: &HashMap<String, Value>) -> LiveCheck {
        let program = match parser::parse(src) {
            Ok(p) => p,
            Err(e) => return LiveCheck::Invalid(e),
        };
        match vm::probe_turn_with_compiled_funcs(
            &program,
            player_vars,
            monster.spec.cycle_budget,
            player.life_points,
            player.max_life,
            monster.life,
            monster.spec.max_life,
            monster.posture,
            monster.spec.weakness,
            Some(&save.loadout),
            save.player_class,
            Some(&save.bag),
            &save.compiled_funcs,
        ) {
            Ok(p) => LiveCheck::Valid { cycles_used: p.cycles_used, truncated: p.truncated },
            Err(e) => LiveCheck::Invalid(e),
        }
    }

    /// `current_phase`: `Some(indice)` na sequência linear de fases
    /// (RFC-005/RFC-026 regra 3), `None` no mapa livre de debug
    /// (`OverworldScene`), que não tem noção de "fase da pirâmide".
    pub fn draw(&self, assets: &Assets, player: &Entity, monster: &MonsterState, foe_kind: Kind, save: &SaveData, current_phase: Option<usize>) {
        clear_background(theme::TUMBA);
        self.draw_top_bar(assets, monster, current_phase);
        self.draw_editor_column(assets);
        self.draw_autocomplete_panel(assets);
        self.draw_arena(assets, player, monster, foe_kind);
        self.draw_dossier_and_log(assets, monster);
        if self.show_load_menu {
            self.draw_load_overlay(assets, save);
        }
        if self.show_rehearsal {
            if let Some(report) = &self.rehearsal {
                self.draw_rehearsal_overlay(assets, report);
            }
        }
        if self.show_compile_choice {
            self.draw_compile_choice_overlay(assets);
        }
    }

    fn draw_top_bar(&self, assets: &Assets, monster: &MonsterState, current_phase: Option<usize>) {
        draw_rectangle(0.0, 0.0, WIDTH, TOP_BAR_H, theme::PEDRA);
        draw_rectangle(0.0, TOP_BAR_H - 3.0, WIDTH, 3.0, theme::OURO);

        draw_text_ex(monster.spec.room, 20.0, 30.0, TextParams { font: Some(&assets.font_title), font_size: 13, color: theme::OURO, ..Default::default() });
        // RFC-026 regra 3: "FASE N/7" ao lado do nome da sala — só quando
        // esta cena está rodando dentro da progressão linear (`PhaseScene`).
        // `current_phase` já veio validado (`PHASES.get` não devolveu
        // `None`), então não precisa tratar "piramide concluida" aqui: esse
        // caso não chega a montar um `DuelScene` (ver `phase.rs::Inner`).
        if let Some(phase_index) = current_phase {
            let room_dims = measure_text(monster.spec.room, Some(&assets.font_title), 13, 1.0);
            draw_text_ex(
                format!("FASE {}/{}", phase_index + 1, crate::monsters::PHASES.len()),
                20.0 + room_dims.width + 14.0,
                30.0,
                TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
            );
        }
        draw_text_ex(
            format!("TURNO {:02}", self.turn),
            20.0,
            52.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::POEIRA, ..Default::default() },
        );

        // RFC-018: fora de Phase::Executing (onde o resultado real de
        // EXECUTAR já existe), o número mostrado é sempre o da última
        // validação ao vivo (`self.live_check`, recalculada a cada frame em
        // `update()`) — nunca mais uma heurística de contagem de linha.
        // Erro de parse/validação não tem "ciclos" reais pra mostrar; 0 é
        // honesto (o script não roda) e a barra de erro abaixo já comunica
        // o motivo.
        //
        // `over` não é só `cost > budget`: a VM nunca deixa `cycles_used`
        // passar do orçamento (`Vm::charge` recusa a última carga em vez de
        // estourar o contador — ver `script/vm.rs`), então um script
        // truncado quase sempre mostra `cycles_used <= budget` mesmo tendo
        // estourado. RFC-018 regra 4 exige o alerta mesmo assim para a
        // validação ao vivo, então `truncated` entra direto na condição em
        // vez de depender só da comparação aritmética. (`Phase::Executing`
        // mantém a comparação pré-existente — não é o que esta RFC pediu
        // pra mudar; ver nota de dívida na entrega.)
        let (cost, over) = match &self.phase {
            Phase::Writing | Phase::Error(_) => match &self.live_check {
                LiveCheck::Valid { cycles_used, truncated } => (*cycles_used, *truncated),
                LiveCheck::Invalid(_) => (0, false),
            },
            Phase::Executing { result, .. } => (result.cycles_used, result.cycles_used > monster.spec.cycle_budget),
        };
        let budget = monster.spec.cycle_budget;
        let cyc_x = WIDTH - 280.0;
        draw_text_ex(
            "CICLOS",
            cyc_x,
            30.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );
        draw_rectangle(cyc_x + 70.0, 16.0, 150.0, 20.0, theme::TUMBA);
        let ratio = (cost as f32 / budget.max(1) as f32).clamp(0.0, 1.0);
        // regra 1: o preenchimento herda o mesmo alerta que já colore o
        // número — antes só o texto virava SANGUE, a barra cheia (o
        // elemento mais visível) ficava muda sobre o estouro.
        let fill_color = if over { theme::SANGUE } else { theme::ESCARAVELHO };
        draw_rectangle(cyc_x + 70.0, 16.0, 150.0 * ratio, 20.0, fill_color);
        draw_rectangle_lines(cyc_x + 70.0, 16.0, 150.0, 20.0, 2.0, theme::AREIA_ESCURA);
        draw_text_ex(
            format!("{cost}/{budget}"),
            cyc_x + 230.0,
            31.0,
            TextParams { font: Some(&assets.font_title), font_size: 12, color: if over { theme::SANGUE } else { theme::PAPIRO }, ..Default::default() },
        );
    }

    fn draw_editor_column(&self, assets: &Assets) {
        let box_y = TOP_BAR_H + 10.0;
        let box_h = 300.0;

        draw_rectangle(4.0, box_y + 4.0, EDITOR_W - 20.0, box_h, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_rectangle(0.0, box_y, EDITOR_W - 20.0, box_h, theme::TUMBA);
        draw_rectangle_lines(0.0, box_y, EDITOR_W - 20.0, box_h, 3.0, theme::OURO);

        // RFC-032 regra 1: a barra virou 2 linhas (`EDITOR_HEADER_H`, era
        // 32px fixos) -- linha 1 (nome do arquivo + contagem) e linha 2
        // (botões, ver EDITOR_HEADER_BUTTONS_Y na construção dos botões)
        // nunca competem pelo mesmo eixo X, então não colidem por
        // construção, qualquer que seja a largura de "{N} LINHAS".
        draw_rectangle(0.0, box_y, EDITOR_W - 20.0, EDITOR_HEADER_H, theme::PEDRA);
        draw_text_ex("turno.pii", 10.0, box_y + 21.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() });
        // Prova numérica (Silkscreen-Regular, medida real via hmtx do
        // .ttf): "turno.pii" a BODY_MD=16 tem 90.0px de largura, termina em
        // x=100.0. `lines_label` right-aligned a 10px da borda direita do
        // header (EDITOR_W - 20.0) cobre até "999 LINHAS" (pior caso de 3
        // dígitos) a BODY_SM=14 = 94.5px, começando em x=440-10-94.5=335.5
        // — 235.5px de sobra até x=100.0, bem acima do gap mínimo de 8px
        // exigido pela regra 1. Por ser calculado com a largura real
        // (`measure_text`) em vez de uma posição fixa, o resultado vale
        // pra qualquer contagem de linhas, não só o pior caso medido aqui.
        let lines_label = format!("{} LINHAS", self.editor.lines.len());
        let lines_dims = measure_text(&lines_label, Some(&assets.font_body), theme::BODY_SM, 1.0);
        let lines_x = (EDITOR_W - 20.0) - 10.0 - lines_dims.width;
        draw_text_ex(
            &lines_label,
            lines_x,
            box_y + 20.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );
        self.btn_clear.draw(&assets.font_body);
        self.btn_save_script.draw(&assets.font_body);
        self.btn_load_script.draw(&assets.font_body);

        self.draw_code_lines(assets, box_y + EDITOR_HEADER_H + 4.0, box_h - EDITOR_HEADER_H - 4.0 - 32.0);

        let (err_bg, err_border, err_color, err_text) = self.error_bar_style();
        let bar_y = box_y + box_h - 32.0;
        draw_rectangle(0.0, bar_y, EDITOR_W - 20.0, 32.0, err_bg);
        draw_rectangle(0.0, bar_y, EDITOR_W - 20.0, 3.0, err_border);
        draw_text_ex(&err_text, 10.0, bar_y + 21.0, TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: err_color, ..Default::default() });

        self.draw_command_palette(assets, box_y + box_h + 12.0);

        self.btn_execute.draw(&assets.font_title);
        self.btn_rehearse.draw(&assets.font_title);
        self.btn_leave.draw(&assets.font_title);
    }

    fn error_bar_style(&self) -> (Color, Color, Color, String) {
        // RFC-018: erro real, seja o de uma execução passada
        // (`Phase::Error`, inalterado) ou o da validação ao vivo do texto
        // atual (`self.live_check` em `Phase::Writing`) — nunca mais
        // "SINTAXE OK" fixo enquanto o jogador digita algo inválido.
        // Estourar orçamento na validação (regra 4) NÃO é erro de
        // sintaxe: continua caindo no braço "ok" abaixo, já que a barra de
        // ciclos (`over`) é quem sinaliza isso.
        let live_error = match &self.phase {
            Phase::Writing => match &self.live_check {
                LiveCheck::Invalid(e) => Some(e),
                LiveCheck::Valid { .. } => None,
            },
            _ => None,
        };
        match (&self.phase, live_error) {
            // regra 6: SANGUE e exclusiva de dano/erro; o texto de erro
            // agora usa a cor do contrato em vez de um tom pastel a parte.
            (Phase::Error(e), _) => (theme::DANGER_BG, theme::SANGUE, theme::SANGUE, format!("{e}")),
            (_, Some(e)) => (theme::DANGER_BG, theme::SANGUE, theme::SANGUE, format!("{e}")),
            // regra 7: MUSGO fica exclusiva do token de valor no editor —
            // "sintaxe ok" e um estado de sucesso, isso e VIDA.
            _ => (theme::OK_BG, theme::TIJOLO, theme::VIDA, "SINTAXE OK - PRONTO PARA EXECUTAR".to_string()),
        }
    }

    fn draw_code_lines(&self, assets: &Assets, y0: f32, h: f32) {
        let line_h = 22.0;
        let max_lines = (h / line_h).floor() as usize;
        // regra 4: `ScriptError::line` é 1-indexado (mesma convenção de
        // `TurnEvent`, ver `event_line` abaixo) — ajusta pro índice do
        // vetor de linhas do editor, que é 0-indexado.
        let error_line = match &self.phase {
            Phase::Error(e) => Some(e.line.saturating_sub(1)),
            _ => None,
        };
        for (i, line) in self.editor.lines.iter().enumerate().take(max_lines) {
            let y = y0 + i as f32 * line_h + 16.0;
            if Some(i) == self.editor.highlighted_line {
                draw_rectangle(0.0, y - 16.0, EDITOR_W - 20.0, line_h, Color::new(0.95, 0.8, 0.3, 0.15));
            } else if Some(i) == error_line {
                // mesma máscara de destaque da linha em execução, mas na
                // cor de erro do contrato (SANGUE a 15%) — Phase::Error e
                // Phase::Executing nunca coexistem, então não há conflito.
                draw_rectangle(0.0, y - 16.0, EDITOR_W - 20.0, line_h, Color::new(theme::SANGUE.r, theme::SANGUE.g, theme::SANGUE.b, 0.15));
            }
            draw_text_ex(
                format!("{:02}", i + 1),
                6.0,
                y,
                TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() },
            );
            // RFC-032 regra 3: só o texto do script (tokens desta linha e a
            // medição do cursor, que precisa da mesma fonte pra não
            // desalinhar do texto desenhado) usa `font_code` — o número da
            // linha acima continua em `font_body`, é rótulo de UI, não
            // código do jogador.
            let mut x = 34.0;
            for (token, color) in highlight_line(line) {
                let dims = measure_text(&token, Some(&assets.font_code), 16, 1.0);
                draw_text_ex(&token, x, y, TextParams { font: Some(&assets.font_code), font_size: 16, color, ..Default::default() });
                x += dims.width;
            }
            if i == self.editor.cursor_row && matches!(self.phase, Phase::Writing) && (get_time() * 2.0) as i64 % 2 == 0 {
                let prefix: String = line.chars().take(self.editor.cursor_col).collect();
                let dims = measure_text(&prefix, Some(&assets.font_code), 16, 1.0);
                draw_rectangle(34.0 + dims.width, y - 14.0, 2.0, 18.0, theme::PAPIRO);
            }
        }
    }

    /// RFC-033 regra 2: posição de tela (x, y da linha de base) do cursor de
    /// texto -- mesma fórmula que o retângulo do cursor em `draw_code_lines`
    /// usa (`font_code`, RFC-032 regra 3), duplicada aqui de propósito: as
    /// duas precisam concordar pixel a pixel para a lista de sugestões
    /// nascer exatamente onde o cursor está, mas vivem em métodos
    /// diferentes porque só um dos dois (este) roda quando não há cursor
    /// nenhum para desenhar (fora de `Phase::Writing`). `None` se a linha do
    /// cursor está fora da área visível do editor (sem rolagem, mesma
    /// limitação que já existe para o próprio cursor em `draw_code_lines`).
    fn cursor_screen_pos(&self, assets: &Assets) -> Option<Vec2> {
        let line_h = 22.0;
        let y0 = EDITOR_BOX_Y + EDITOR_HEADER_H + 4.0;
        let h = EDITOR_BOX_H - EDITOR_HEADER_H - 4.0 - 32.0;
        let max_lines = (h / line_h).floor() as usize;
        let row = self.editor.cursor_row;
        if row >= max_lines {
            return None;
        }
        let line = self.editor.lines.get(row)?;
        let prefix: String = line.chars().take(self.editor.cursor_col).collect();
        let dims = measure_text(&prefix, Some(&assets.font_code), 16, 1.0);
        Some(vec2(34.0 + dims.width, y0 + row as f32 * line_h + 16.0))
    }

    /// RFC-033 regra 2: lista pequena (até 5) ancorada perto do cursor,
    /// reaproveitando o mesmo trio de cores dos cartões da paleta de
    /// comandos (`PEDRA`/`TIJOLO`/`OURO`) -- sem cor nova. A sugestão
    /// destacada ganha a descrição do comando embaixo da lista quando ela
    /// existe (regra 3), mesmo texto que `draw_command_palette` mostra no
    /// hover. Também é o único lugar que escreve em `autocomplete_rects` --
    /// `update()` (no frame seguinte) lê de lá pra saber se um clique
    /// acertou uma linha (ver o campo, documentado em `DuelScene`).
    fn draw_autocomplete_panel(&self, assets: &Assets) {
        let show = matches!(self.phase, Phase::Writing) && self.autocomplete.is_some();
        if !show {
            self.autocomplete_rects.borrow_mut().clear();
            return;
        }
        let ac = self.autocomplete.as_ref().unwrap();
        let Some(anchor) = self.cursor_screen_pos(assets) else {
            self.autocomplete_rects.borrow_mut().clear();
            return;
        };

        const ROW_H: f32 = 22.0;
        const PAD: f32 = 8.0;
        const DESC_LINE_H: f32 = 16.0;
        // regra 2: lista pequena o bastante que o nome mais longo (nunca
        // maior que "for i in a..b:") cabe folgado num painel estreito --
        // largura decidida só pelos nomes; a descrição (bem mais longa)
        // quebra em linhas dentro dessa mesma largura em vez de alargar o
        // painel até quase a coluna inteira do editor.
        let name_width = ac.candidates.iter().map(|c| measure_text(&c.name, Some(&assets.font_body), 14, 1.0).width).fold(0.0_f32, f32::max);
        let width = (name_width + PAD * 2.0).clamp(120.0, 220.0);
        let desc = ac.candidates[ac.selected].description;
        let desc_lines = desc.map(|t| wrap_text_px(t, &assets.font_body, 12, width - PAD * 2.0)).unwrap_or_default();
        let desc_h = if desc_lines.is_empty() { 0.0 } else { 6.0 + desc_lines.len() as f32 * DESC_LINE_H };
        let list_h = ac.candidates.len() as f32 * ROW_H;
        let panel_h = list_h + desc_h;

        // regra 2: ancorado perto do cursor, logo abaixo da linha atual --
        // clampeado pra nunca vazar a coluna do editor pela direita.
        let origin_x = (anchor.x).min(EDITOR_W - 20.0 - width);
        let origin_y = anchor.y + 6.0;
        let origin = vec2(origin_x, origin_y);

        draw_rectangle(origin.x - 2.0, origin.y - 2.0, width + 4.0, panel_h + 4.0, theme::PEDRA);
        draw_rectangle_lines(origin.x - 2.0, origin.y - 2.0, width + 4.0, panel_h + 4.0, 2.0, theme::OURO);

        let mut rects = Vec::with_capacity(ac.candidates.len());
        for (i, cand) in ac.candidates.iter().enumerate() {
            let r = Self::autocomplete_row_rect(origin, i, width);
            if i == ac.selected {
                draw_rectangle(r.x, r.y, r.w, r.h, theme::AREIA_ESCURA);
            }
            draw_text_ex(
                &cand.name,
                r.x + PAD,
                r.y + 16.0,
                TextParams { font: Some(&assets.font_body), font_size: 14, color: if i == ac.selected { theme::OURO } else { theme::ESCARAVELHO }, ..Default::default() },
            );
            rects.push(r);
        }
        for (i, line) in desc_lines.iter().enumerate() {
            let y = origin.y + list_h + 12.0 + i as f32 * DESC_LINE_H;
            draw_text_ex(line, origin.x + PAD, y, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() });
        }

        *self.autocomplete_rects.borrow_mut() = rects;
    }

    fn draw_command_palette(&self, assets: &Assets, panel_y: f32) {
        draw_text_ex(
            "COMANDOS - CLIQUE PARA INSERIR",
            10.0,
            panel_y + 16.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );
        for (i, cmd) in COMMANDS.iter().enumerate() {
            let r = Self::command_rect(i);
            let card = &self.command_cards[i];
            // regra 3: flash de clique (OURO) tem prioridade sobre hover
            // (POEIRA), que tem prioridade sobre o repouso (TIJOLO).
            let border = if card.flash.is_some() {
                theme::OURO
            } else if card.hovered {
                theme::POEIRA
            } else {
                theme::TIJOLO
            };
            draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, border);
            draw_text_ex(cmd.label, r.x + 8.0, r.y + 21.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::ESCARAVELHO, ..Default::default() });
            let cost_dims = measure_text(cmd.cost_label, Some(&assets.font_body), 12, 1.0);
            draw_text_ex(
                cmd.cost_label,
                r.x + r.w - cost_dims.width - 8.0,
                r.y + 21.0,
                TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::POEIRA, ..Default::default() },
            );
        }

        // RFC-033 regra 3: tooltip simples no hover de um cartão -- desenhado
        // depois de todos os cartões (por cima, nunca atrás de outro
        // cartão). No máximo um cartão está `hovered` por vez (o mouse só
        // pode estar sobre um retângulo), então não há ambiguidade sobre
        // qual descrição mostrar.
        if let Some((i, cmd)) = COMMANDS.iter().enumerate().find(|(i, _)| self.command_cards[*i].hovered) {
            let r = Self::command_rect(i);
            let lines = wrap_text_px(cmd.description, &assets.font_body, 12, r.w - 16.0);
            let tip_h = 8.0 + lines.len() as f32 * 16.0;
            let tip_y = r.y - tip_h - 4.0;
            draw_rectangle(r.x, tip_y, r.w, tip_h, theme::PEDRA);
            draw_rectangle_lines(r.x, tip_y, r.w, tip_h, 2.0, theme::OURO);
            for (li, line) in lines.iter().enumerate() {
                draw_text_ex(line, r.x + 8.0, tip_y + 16.0 + li as f32 * 16.0, TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::PAPIRO, ..Default::default() });
            }
        }
    }

    /// RFC-026 regra 2: lista de `save.scripts` sobreposta à tela inteira,
    /// aberta pelo botão CARREGAR. Mesmo cartão visual da paleta de
    /// comandos (`draw_command_palette` acima) — reaproveitado em vez de
    /// inventar um componente novo, mesmo raciocínio que a RFC pede.
    fn draw_load_overlay(&self, assets: &Assets, save: &SaveData) {
        draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.6));

        draw_text_ex(
            "CARREGAR SCRIPT - CLIQUE PARA SUBSTITUIR O EDITOR (ESC PARA FECHAR)",
            WIDTH * 0.2,
            86.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() },
        );

        for (i, script) in save.scripts.iter().enumerate() {
            let r = Self::load_card_rect(i);
            let hovered = r.contains(virtual_mouse_position().into());
            let border = if hovered { theme::POEIRA } else { theme::TIJOLO };
            draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, border);
            draw_text_ex(&script.name, r.x + 12.0, r.y + 22.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::ESCARAVELHO, ..Default::default() });
            let lines = script.body.lines().count();
            let preview = script.body.lines().next().unwrap_or("");
            let summary = format!("{lines} linha(s) - {preview}");
            draw_text_ex(&summary, r.x + 12.0, r.y + 42.0, TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() });
        }
    }

    /// RFC-027: painel do Ensaio Geral — lista compacta por turno (regra 4),
    /// mais o desfecho da simulação. Não reaproveita `draw_log` de
    /// propósito (não-objetivo da RFC): é informação de um ensaio, não o
    /// registro do duelo real, e merecia ficar visualmente distinto pra não
    /// ser confundido com um resultado que já aconteceu de verdade.
    fn draw_rehearsal_overlay(&self, assets: &Assets, report: &rehearsal::RehearsalReport) {
        draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.7));

        let panel_x = WIDTH * 0.15;
        let panel_w = WIDTH * 0.7;
        let panel_y = 80.0;
        let panel_h = HEIGHT - panel_y - 40.0;
        draw_rectangle(panel_x, panel_y, panel_w, panel_h, theme::PEDRA);
        draw_rectangle_lines(panel_x, panel_y, panel_w, panel_h, 3.0, theme::OURO);

        draw_text_ex(
            "ENSAIO GERAL - SIMULACAO SOBRE UM CLONE (ESC OU ENSAIAR PARA FECHAR)",
            panel_x + 16.0,
            panel_y + 28.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() },
        );

        let (end_text, end_color) = match &report.end {
            RehearsalEnd::MonsterDied => ("VITORIA SIMULADA".to_string(), theme::VIDA),
            RehearsalEnd::PlayerDied => ("DERROTA SIMULADA".to_string(), theme::SANGUE),
            RehearsalEnd::TurnCapReached => (format!("TETO DE {} TURNOS ATINGIDO - O DUELO NAO FECHOU", rehearsal::REHEARSAL_TURN_CAP), theme::CHAMA),
            RehearsalEnd::Error(e) => (format!("ERRO NO MEIO DA SIMULACAO: {e}"), theme::SANGUE),
        };
        draw_text_ex(
            format!("{end_text} - {} turno(s) simulado(s)", report.turns.len()),
            panel_x + 16.0,
            panel_y + 52.0,
            TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_SM, color: end_color, ..Default::default() },
        );

        let row_h = 20.0;
        let list_y0 = panel_y + 76.0;
        let max_rows = ((panel_h - 76.0 - 16.0) / row_h).floor() as usize;
        for (i, t) in report.turns.iter().take(max_rows).enumerate() {
            let y = list_y0 + i as f32 * row_h;
            let color = if t.truncated { theme::SANGUE } else { theme::POEIRA };
            let line = format!(
                "turno {:02}: {} dano causado - ciclos {}/{}{} - {} no jogador",
                t.turn,
                t.damage_dealt,
                t.cycles_used,
                t.cycle_budget,
                if t.truncated { " (estourou)" } else { "" },
                if t.damage_taken >= 0 { format!("-{}", t.damage_taken) } else { format!("+{}", -t.damage_taken) },
            );
            draw_text_ex(&line, panel_x + 16.0, y + 16.0, TextParams { font: Some(&assets.font_body), font_size: 14, color, ..Default::default() });
        }
        if report.turns.len() > max_rows {
            let y = list_y0 + max_rows as f32 * row_h;
            draw_text_ex(
                format!("... e mais {} turno(s) (nao exibidos)", report.turns.len() - max_rows),
                panel_x + 16.0,
                y + 16.0,
                TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::POEIRA, ..Default::default() },
            );
        }
    }

    /// RFC-030 regra 2: tela de escolha pós-vitória -- reaproveita o mesmo
    /// cartão visual/layout de `draw_load_overlay` (RFC-026) em vez de
    /// inventar um componente novo, exatamente como a RFC pede. Um cartão
    /// por nome de `func` do script vencedor, mais um cartão "PULAR" ao
    /// final -- escolher "nenhuma" é sempre uma opção válida (regra 5).
    fn draw_compile_choice_overlay(&self, assets: &Assets) {
        draw_rectangle(0.0, 0.0, WIDTH, HEIGHT, Color::new(0.0, 0.0, 0.0, 0.7));

        draw_text_ex(
            "FASE VENCIDA - ESCOLHA 1 FUNCAO PRA COMPILAR (DE GRACA PRA SEMPRE, POR NOME) OU PULE (ESC)",
            WIDTH * 0.2,
            86.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() },
        );
        draw_text_ex(
            "A isenção vale pra qualquer futura 'func' com este nome, mesmo redefinida.",
            WIDTH * 0.2,
            104.0,
            TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() },
        );

        let mouse: Vec2 = virtual_mouse_position().into();
        for (i, name) in self.compile_choice_names.iter().enumerate() {
            let r = Self::compile_choice_card_rect(i);
            let hovered = r.contains(mouse);
            let border = if hovered { theme::OURO } else { theme::TIJOLO };
            draw_rectangle(r.x, r.y, r.w, r.h, theme::PEDRA);
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, border);
            draw_text_ex(
                format!("func {name}()"),
                r.x + 12.0,
                r.y + 22.0,
                TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_SM, color: theme::ESCARAVELHO, ..Default::default() },
            );
            draw_text_ex(
                "compilar -- chamadas futuras deste nome custam 0 ciclo",
                r.x + 12.0,
                r.y + 42.0,
                TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::POEIRA, ..Default::default() },
            );
        }

        let skip = Self::compile_choice_skip_rect(self.compile_choice_names.len());
        let skip_hovered = skip.contains(mouse);
        draw_rectangle(skip.x, skip.y, skip.w, skip.h, theme::PEDRA);
        draw_rectangle_lines(skip.x, skip.y, skip.w, skip.h, 2.0, if skip_hovered { theme::POEIRA } else { theme::TIJOLO });
        draw_text_ex("NENHUMA (PULAR)", skip.x + 12.0, skip.y + 30.0, TextParams { font: Some(&assets.font_body), font_size: 16, color: theme::POEIRA, ..Default::default() });
    }

    fn draw_arena(&self, assets: &Assets, player: &Entity, monster: &MonsterState, foe_kind: Kind) {
        draw_texture_ex(
            &assets.bg_dungeon,
            ARENA_X,
            TOP_BAR_H,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(ARENA_W, HEIGHT - TOP_BAR_H)), ..Default::default() },
        );
        draw_rectangle(ARENA_X, TOP_BAR_H, ARENA_W, HEIGHT - TOP_BAR_H, Color::new(0.0, 0.0, 0.0, 0.18));

        draw_hp_row(assets, ARENA_X + 20.0, TOP_BAR_H + 16.0, "VOCE", player.life_points, player.max_life, true);
        draw_hp_row(assets, ARENA_X + ARENA_W - 20.0, TOP_BAR_H + 16.0, monster.spec.title, monster.life, monster.spec.max_life, false);

        let t = get_time() as f32;
        let hero_bob = (t * 1.8).sin() * 4.0;
        let hero_size = 120.0;
        // RFC-020: recuo (shake) + lunge somados ao x parado — somar em vez
        // de substituir é a mitigação de risco que a própria RFC lista
        // ("brigar visualmente com o idle bob"): o bob vertical continua
        // intocado, só o eixo horizontal ganha o movimento reativo, então
        // não competem pelo mesmo eixo.
        let hero_hit_shake = self.hero_anim.hit.as_ref().map_or(0.0, HitReaction::shake_px);
        let hero_lunge_x = self.hero_anim.lunge.as_ref().map_or(0.0, |l| l.offset_px(1.0));
        let hero_x = ARENA_X + 60.0 + hero_hit_shake + hero_lunge_x;
        let hero_y = HEIGHT - 170.0 + hero_bob;
        draw_rectangle(hero_x, hero_y, hero_size, hero_size, theme::PEDRA);
        draw_rectangle_lines(hero_x, hero_y, hero_size, hero_size, 3.0, theme::OURO);
        draw_texture_ex(
            assets.portrait_for(Kind::Player),
            hero_x,
            hero_y,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(hero_size, hero_size)), ..Default::default() },
        );
        if let Some(hit) = &self.hero_anim.hit {
            let alpha = hit.tint_alpha_now();
            if alpha > 0.0 {
                draw_rectangle(hero_x, hero_y, hero_size, hero_size, Color::new(theme::SANGUE.r, theme::SANGUE.g, theme::SANGUE.b, alpha));
            }
        }

        let foe_bob = (t * 1.3 + 1.0).sin() * 5.0;
        let foe_size = 170.0;
        let foe_hit_shake = self.foe_anim.hit.as_ref().map_or(0.0, HitReaction::shake_px);
        let foe_lunge_x = self.foe_anim.lunge.as_ref().map_or(0.0, |l| l.offset_px(-1.0));
        let foe_x = ARENA_X + ARENA_W - foe_size - 60.0 + foe_hit_shake + foe_lunge_x;
        let foe_y = HEIGHT - 260.0 + foe_bob;
        draw_rectangle(foe_x, foe_y, foe_size, foe_size, theme::PEDRA);
        // regra 6: identidade do inimigo (moldura do retrato) e neutra —
        // SANGUE fica exclusiva de dano/erro, nao de "isto e o inimigo".
        draw_rectangle_lines(foe_x, foe_y, foe_size, foe_size, 3.0, theme::TIJOLO);
        draw_texture_ex(
            assets.portrait_for(foe_kind),
            foe_x,
            foe_y,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(foe_size, foe_size)), ..Default::default() },
        );
        if let Some(hit) = &self.foe_anim.hit {
            let alpha = hit.tint_alpha_now();
            if alpha > 0.0 {
                draw_rectangle(foe_x, foe_y, foe_size, foe_size, Color::new(theme::SANGUE.r, theme::SANGUE.g, theme::SANGUE.b, alpha));
            }
        }
        let tag = monster.spec.weakness.label();
        let weakness_icon = assets.icon_for_weakness(monster.spec.weakness);
        // RFC-013: selo ao lado da tag para as 4 fraquezas com ícone
        // desenhado; DuploSelo/ExigeNomeacao seguem só com o texto.
        let icon_gap = if weakness_icon.is_some() { 22.0 } else { 0.0 };
        let tag_dims = measure_text(tag, Some(&assets.font_body), 13, 1.0);
        // regra 4 + 6: AREIA_ESCURA em seu papel real (preenchimento de
        // superficie elevada) para a etiqueta de fraqueza, em vez de
        // SANGUE (que agora e exclusiva de dano/erro).
        draw_rectangle(foe_x - 4.0, foe_y - 4.0, tag_dims.width + 12.0 + icon_gap, 22.0, theme::AREIA_ESCURA);
        if let Some(icon) = weakness_icon {
            draw_texture_ex(icon, foe_x + 2.0, foe_y - 2.0, WHITE, DrawTextureParams { dest_size: Some(vec2(18.0, 18.0)), ..Default::default() });
        }
        draw_text_ex(tag, foe_x + 2.0 + icon_gap, foe_y + 12.0, TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::PAPIRO, ..Default::default() });

        if let Some(hit) = &self.hit {
            let progress = (hit.timer / 1.1).clamp(0.0, 1.0);
            let rise = progress * 64.0;
            let alpha = (1.0 - progress).clamp(0.0, 1.0);
            // dano no popup flutuante e o papel canonico de SANGUE.
            let color = if hit.special { theme::OURO } else { Color::new(theme::SANGUE.r, theme::SANGUE.g, theme::SANGUE.b, alpha) };
            let label = format!("-{}", hit.value);
            draw_text_ex(
                &label,
                foe_x + foe_size * 0.3,
                foe_y - 20.0 - rise,
                TextParams { font: Some(&assets.font_title), font_size: 30, color, ..Default::default() },
            );
        }

        self.draw_intent_bar(assets, monster);
    }

    fn draw_intent_bar(&self, assets: &Assets, monster: &MonsterState) {
        let y = HEIGHT - 46.0;
        draw_rectangle(ARENA_X + 20.0, y, ARENA_W - 40.0, 34.0, Color::new(0.05, 0.03, 0.02, 0.9));
        draw_rectangle_lines(ARENA_X + 20.0, y, ARENA_W - 40.0, 34.0, 2.0, theme::TIJOLO);

        let special = monster.special_ready();
        let intent = if special { monster.spec.special_attack_name } else { monster.spec.attack_name };
        draw_text_ex(
            "PROXIMA ACAO",
            ARENA_X + 30.0,
            y + 14.0,
            TextParams { font: Some(&assets.font_body), font_size: 12, color: theme::ESCARAVELHO, ..Default::default() },
        );
        draw_text_ex(
            intent.to_uppercase(),
            ARENA_X + 30.0,
            y + 29.0,
            TextParams { font: Some(&assets.font_body), font_size: 15, color: if special { theme::OURO } else { theme::PAPIRO }, ..Default::default() },
        );

        let bar_w = 140.0;
        let bar_x = ARENA_X + ARENA_W - 20.0 - bar_w;
        // regra 9: trilha vazia continua TUMBA.
        draw_rectangle(bar_x, y + 8.0, bar_w, 16.0, theme::TUMBA);
        let ratio = (monster.charge as f32 / crate::monsters::CHARGE_THRESHOLD as f32).clamp(0.0, 1.0);
        // regra 2: CHAMA e exclusiva de carga/alerta — sai do destaque
        // de sintaxe e passa a preencher a barra de carga.
        draw_rectangle(bar_x, y + 8.0, bar_w * ratio, 16.0, theme::CHAMA);
        // regra 8: com a carga cheia, alem da cor, um segundo canal por
        // movimento — moldura mais espessa e piscando em cadencia lenta
        // entre CHAMA/OURO, deterministico pelo tempo (mesmo padrao do
        // "idle bob" dos retratos). Funciona mesmo pra quem nao distingue
        // as duas cores: a moldura muda de espessura e de brilho.
        let (border_color, border_w) = if special {
            let blink_on = (get_time() * 0.6) as i64 % 2 == 0;
            (if blink_on { theme::CHAMA } else { theme::OURO }, 4.0)
        } else {
            (theme::AREIA_ESCURA, 2.0)
        };
        draw_rectangle_lines(bar_x, y + 8.0, bar_w, 16.0, border_w, border_color);
    }

    fn draw_dossier_and_log(&self, assets: &Assets, monster: &MonsterState) {
        let x = WIDTH - SIDE_W;
        let mut y = TOP_BAR_H + 10.0;
        let card_h = 300.0;

        draw_rectangle(x + 4.0, y + 4.0, SIDE_W - 20.0, card_h, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_rectangle(x, y, SIDE_W - 20.0, card_h, theme::PEDRA);
        draw_rectangle_lines(x, y, SIDE_W - 20.0, card_h, 3.0, theme::OURO);
        y += 16.0;

        draw_text_ex(monster.spec.title.to_uppercase(), x + 12.0, y + 18.0, TextParams { font: Some(&assets.font_title), font_size: theme::TITLE_MD, color: theme::PAPIRO, ..Default::default() });
        y += 34.0;
        let posture_label = format!("POSTURA: {}", monster.posture.as_str().to_uppercase());
        draw_text_ex(
            &posture_label,
            x + 12.0,
            y,
            // regra 3: postura e dado primario, nao destaque — sai de
            // OURO e passa a PAPIRO.
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_MD, color: theme::PAPIRO, ..Default::default() },
        );
        // RFC-013: selo de postura ao lado do texto — dado que a fraqueza
        // do Escaravelho depende de ler corretamente turno a turno.
        let posture_dims = measure_text(&posture_label, Some(&assets.font_body), theme::BODY_MD, 1.0);
        draw_texture_ex(
            assets.icon_for_posture(monster.posture),
            x + 12.0 + posture_dims.width + 8.0,
            y - 15.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(18.0, 18.0)), ..Default::default() },
        );
        y += 22.0;
        draw_text_ex(
            format!("CARGA {}/{}", monster.charge.min(crate::monsters::CHARGE_THRESHOLD), crate::monsters::CHARGE_THRESHOLD),
            x + 12.0,
            y,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );
        y += 20.0;

        for line in monster.spec.description {
            for wrapped in wrap_text(line, 26) {
                draw_text_ex(&wrapped, x + 12.0, y, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::POEIRA, ..Default::default() });
                y += 17.0;
            }
        }
        // achado #5 da auditoria de QoL: `Weakness::Eficiencia` era a unica
        // fraqueza que nunca mostrava o numero de verdade que ela mede --
        // o resto do dossie e concreto (postura em texto exato, carga em
        // fracao N/20, vida em N/N), só essa ficava em prosa vaga
        // ("aguenta scripts curtos"). O valor ja existe no dado do monstro
        // (`max_ciclos`), so faltava desenhar.
        if let Weakness::Eficiencia { max_ciclos } = monster.spec.weakness {
            let extra = format!("So aguenta ate {max_ciclos} ciclos de execucao por turno.");
            for wrapped in wrap_text(&extra, 26) {
                draw_text_ex(&wrapped, x + 12.0, y, TextParams { font: Some(&assets.font_body), font_size: 14, color: theme::POEIRA, ..Default::default() });
                y += 17.0;
            }
        }
        y += 8.0;

        let tag = monster.spec.weakness.label();
        let weakness_icon = assets.icon_for_weakness(monster.spec.weakness);
        let icon_gap = if weakness_icon.is_some() { 22.0 } else { 0.0 };
        let tag_dims = measure_text(tag, Some(&assets.font_body), 13, 1.0);
        // regra 6: tag de fraqueza e identidade do inimigo, nao dano/erro
        // — moldura neutra (TIJOLO/AREIA_ESCURA) em vez de SANGUE/DANGER_BG.
        draw_rectangle(x + 12.0, y, tag_dims.width + 16.0 + icon_gap, 24.0, theme::AREIA_ESCURA);
        draw_rectangle_lines(x + 12.0, y, tag_dims.width + 16.0 + icon_gap, 24.0, 2.0, theme::TIJOLO);
        if let Some(icon) = weakness_icon {
            draw_texture_ex(icon, x + 16.0, y + 3.0, WHITE, DrawTextureParams { dest_size: Some(vec2(18.0, 18.0)), ..Default::default() });
        }
        draw_text_ex(tag, x + 20.0 + icon_gap, y + 17.0, TextParams { font: Some(&assets.font_body), font_size: 13, color: theme::PAPIRO, ..Default::default() });
        y += 34.0;

        self.draw_item_icons(assets, x + 12.0, y);

        self.draw_log(assets, x, TOP_BAR_H + 10.0 + card_h + 14.0);
    }

    fn draw_item_icons(&self, assets: &Assets, x: f32, y: f32) {
        let items = [ItemKind::Espada, ItemKind::Magia, ItemKind::Escudo, ItemKind::Pocao];
        for (i, kind) in items.iter().enumerate() {
            let cx = x + i as f32 * 44.0;
            draw_texture_ex(assets.icon_for(*kind), cx, y, WHITE, DrawTextureParams { dest_size: Some(vec2(32.0, 32.0)), ..Default::default() });
        }
    }

    fn draw_log(&self, assets: &Assets, x: f32, y: f32) {
        let h = HEIGHT - y - 10.0;
        draw_rectangle(x, y, SIDE_W - 20.0, h, theme::TUMBA);
        draw_rectangle_lines(x, y, SIDE_W - 20.0, h, 3.0, theme::AREIA_ESCURA);
        draw_rectangle(x, y, SIDE_W - 20.0, 30.0, theme::PEDRA);
        draw_text_ex(
            // achado #6 da auditoria de QoL: o painel acumula o duelo
            // inteiro desde `DuelScene::new()`, nunca so o turno atual --
            // o rotulo antigo ("REGISTRO DO TURNO") prometia o oposto do
            // que a tela mostra. Mudanca de string, sem alterar o corte
            // silencioso de linhas antigas (isso e feature maior, fora do
            // escopo deste achado).
            "REGISTRO DO DUELO",
            x + 10.0,
            y + 20.0,
            TextParams { font: Some(&assets.font_body), font_size: theme::BODY_SM, color: theme::POEIRA, ..Default::default() },
        );

        let visible_h = h - 40.0;
        let max_lines = (visible_h / 20.0).floor() as usize;
        // RFC-032 regra 2: `line` podia ser mais largo que o painel
        // (`SIDE_W`) e cortava sem quebra visível — mesmo `wrap_text` que
        // `draw_dossier_and_log` já usa pra descrição do monstro (26
        // caracteres cabe em `SIDE_W - 20.0` a este mesmo font_size 14,
        // é o valor já validado nesse outro uso da coluna). Cada entrada
        // de `self.log` pode virar mais de uma linha desenhada agora, por
        // isso a lista final é achatada (flatten) em ordem cronológica
        // antes de recortar as últimas `max_lines` que cabem no painel —
        // recortar por entrada (como antes) permitiria uma entrada wrapped
        // ultrapassar o fundo do painel.
        let mut flattened: Vec<(String, Color)> = Vec::new();
        for (line, color) in self.log.iter() {
            for wrapped in wrap_text(line, 26) {
                flattened.push((wrapped, *color));
            }
        }
        let start = flattened.len().saturating_sub(max_lines);
        for (i, (line, color)) in flattened[start..].iter().enumerate() {
            draw_text_ex(line, x + 10.0, y + 48.0 + i as f32 * 20.0, TextParams { font: Some(&assets.font_body), font_size: 14, color: *color, ..Default::default() });
        }
    }
}

fn draw_hp_row(assets: &Assets, x: f32, y: f32, label: &str, life: i32, max_life: i32, align_left: bool) {
    let bar_w = 220.0;
    let bar_x = if align_left { x } else { x - bar_w };
    draw_text_ex(
        label,
        bar_x,
        y + 14.0,
        TextParams { font: Some(&assets.font_title), font_size: 14, color: theme::PAPIRO, ..Default::default() },
    );
    draw_rectangle(bar_x, y + 20.0, bar_w, 20.0, theme::TUMBA);
    let ratio = (life.max(0) as f32 / max_life.max(1) as f32).clamp(0.0, 1.0);
    let color = if align_left { theme::VIDA } else { theme::SANGUE };
    draw_rectangle(bar_x, y + 20.0, bar_w * ratio, 20.0, color);
    draw_rectangle_lines(bar_x, y + 20.0, bar_w, 20.0, 2.0, theme::OURO);
    draw_text_ex(
        format!("{}/{}", life.max(0), max_life),
        bar_x + 4.0,
        y + 35.0,
        TextParams { font: Some(&assets.font_body_bold), font_size: 13, color: theme::PAPIRO, ..Default::default() },
    );
}

const KEYWORDS: &[&str] =
    &["if", "else", "while", "for", "func", "invocar", "in", "and", "or", "not", "e", "ou", "nao", "true", "false"];
const NATIVE_FUNCS: &[&str] = &["atacar", "defender", "inspecionar", "curar", "esperar"];
const COLLECTIONS: &[&str] = &["espada", "magia", "escudo", "pocao", "eu", "inimigo"];

/// tokenizer só-pra-exibição: colore o texto do editor por classe de
/// caractere, sem exigir que ele seja sintaticamente válido (o jogador
/// pode estar no meio de digitar algo incompleto). Não reaproveita
/// `script::lexer` de propósito — aquele exige o texto lexar com
/// sucesso, e perderia formatação exata (espaçamento, aspas) ao
/// reconstituir do token.
/// RFC-033: quebra `text` em linhas que cabem em `max_width` (medidas com
/// `font`/`size`), palavra por palavra -- usado pela descrição do
/// autocomplete e pelo tooltip da paleta de comandos, os dois textos livres
/// desta cena que não cabem numa linha só num painel estreito. Uma única
/// palavra maior que `max_width` sozinha (não deveria acontecer com as
/// descrições curtas de `COMMANDS`, mas não é impossível) vira sua própria
/// linha sem cortar caracteres -- prefere vazar um pouco a truncar texto.
fn wrap_text_px(text: &str, font: &Font, size: u16, max_width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if !current.is_empty() && measure_text(&candidate, Some(font), size, 1.0).width > max_width {
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

fn highlight_line(line: &str) -> Vec<(String, Color)> {
    let mut out = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == ' ' {
            let start = i;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            out.push((chars[start..i].iter().collect(), theme::PAPIRO));
        } else if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            out.push((chars[start..i].iter().collect(), theme::MUSGO));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            out.push((chars[start..i].iter().collect(), theme::MUSGO));
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // enum-style: primeira letra maiuscula (Fogo, Bronze, Vida...)
            // pinta como valor de enum em vez de identificador comum
            let is_enum_value = word.chars().next().is_some_and(|c| c.is_uppercase());
            // regra 2: CHAMA e exclusiva de carga/alerta — palavra-chave
            // de controle passa a ESCARAVELHO (informacao, contraste maior).
            let color = if KEYWORDS.contains(&word.to_lowercase().as_str()) {
                theme::ESCARAVELHO
            } else if is_enum_value {
                theme::MUSGO
            } else if NATIVE_FUNCS.contains(&word.as_str()) || COLLECTIONS.contains(&word.as_str()) {
                theme::ESCARAVELHO
            } else {
                theme::PAPIRO
            };
            out.push((word, color));
        } else if "(){}[]:,".contains(c) {
            out.push((c.to_string(), theme::POEIRA));
            i += 1;
        } else {
            // inclui '.', que separa coleção do valor-enum (magia.Fogo)
            out.push((c.to_string(), theme::OURO));
            i += 1;
        }
    }
    out
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > max_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn event_line(ev: &TurnEvent) -> Option<usize> {
    match ev {
        TurnEvent::Attacked { line, .. }
        | TurnEvent::Defended { line, .. }
        | TurnEvent::Inspected { line }
        | TurnEvent::Healed { line, .. }
        | TurnEvent::Waited { line }
        | TurnEvent::Truncated { line }
        | TurnEvent::Selected { line, .. } => Some(line.saturating_sub(1)),
        TurnEvent::BonusStrike { .. } | TurnEvent::CounterAttack { .. } => None,
    }
}

fn popup_for_event(ev: &TurnEvent) -> Option<HitPopup> {
    match ev {
        TurnEvent::Attacked { damage, .. } if *damage > 0 => Some(HitPopup { value: *damage, special: false, timer: 0.0 }),
        TurnEvent::BonusStrike { damage } => Some(HitPopup { value: *damage, special: true, timer: 0.0 }),
        _ => None,
    }
}

/// capitaliza a primeira letra do nome do item pro log ficar no mesmo
/// estilo "enum" usado no editor (Fogo, Bronze, Vida...)
fn enum_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn describe_event(ev: &TurnEvent) -> (String, Color) {
    match ev {
        TurnEvent::Attacked { item, damage, effective, .. } => {
            let hit = if *effective { "acerto em cheio" } else { "de raspao" };
            // regra 7: MUSGO fica exclusiva de token de valor no editor —
            // ataque efetivo (sucesso) e VIDA; de raspao e neutro (POEIRA).
            let color = if *effective { theme::VIDA } else { theme::POEIRA };
            (format!("atacar({}.{}) -> {hit}, {damage} de dano", item.kind.label(), enum_case(&item.name)), color)
        }
        TurnEvent::Defended { item, .. } => (format!("defender({}.{})", item.kind.label(), enum_case(&item.name)), theme::ESCARAVELHO),
        TurnEvent::Inspected { .. } => ("inspecionar() -> fraqueza revelada".to_string(), theme::ESCARAVELHO),
        TurnEvent::Healed { amount, .. } => (format!("curar() -> +{amount} de vida"), theme::VIDA),
        TurnEvent::Waited { .. } => ("esperar()".to_string(), theme::POEIRA),
        TurnEvent::BonusStrike { damage } => (format!("script eficiente! golpe bonus: {damage} de dano"), theme::OURO),
        TurnEvent::CounterAttack { damage, blocked, special, truncated } => {
            let name = if *special { "golpe especial" } else { "golpe do turno" };
            let suffix = if *blocked { " (bloqueado pela metade)" } else { "" };
            // RFC-025 regra 1/3: o monstro golpeia todo turno agora -- o
            // texto so muda de tom quando o script estourou o orcamento
            // (o golpe fica mais pesado, ver TRUNCATE_DAMAGE_MULTIPLIER).
            if *truncated {
                (format!("orcamento estourou! {name} punido{suffix}: {damage} de dano"), theme::SANGUE)
            } else {
                (format!("a piramide cobra o folego do turno -- {name}{suffix}: {damage} de dano"), theme::SANGUE)
            }
        }
        TurnEvent::Truncated { .. } => ("-- execucao interrompida: ciclos esgotados --".to_string(), theme::SANGUE),
        // RFC-015 regra 10: mostra `examined`, nao so o custo em ciclos --
        // e o numero que ensina por que reordenar `and` dentro de `onde:`
        // muda o custo real. Texto provisorio (designer decide o final).
        TurnEvent::Selected { examined, found, .. } => {
            let plural = if *examined == 1 { "item" } else { "itens" };
            if *found {
                (format!("selecionar() -> item encontrado ({examined} {plural} examinados)"), theme::OURO)
            } else {
                (format!("selecionar() -> nenhum item bateu o filtro ({examined} {plural} examinados)"), theme::POEIRA)
            }
        }
    }
}

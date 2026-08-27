//! A máquina de execução do pseudo-código: interpreta a AST com um
//! orçamento de ciclos, resolve o combate contra a fraqueza do monstro, e
//! devolve uma trilha de eventos que a cena de duelo anima linha a linha.
//!
//! Execução em duas passadas: a primeira roda em modo `dry_run` (sem
//! efeitos colaterais) só para validar — nome de função desconhecida,
//! tipo errado, variável não definida — e devolve erro sem mexer no HP de
//! ninguém. Só depois de validar é que a segunda passada roda de verdade.
//! É assim que um erro de sintaxe/tipo nunca consome o turno do jogador.

use std::collections::HashMap;

use super::api::{self, LOOP_TICK_COST, BRANCH_COST};
use super::ast::{BinOp, Expr, Stmt, StmtKind, UnaryOp};
use super::error::ScriptError;
use super::value::{Item, ItemKind, Target, Value};
use crate::inventory::{Bag, Loadout, PlayerClass};
use crate::monsters::{Element, Posture, Weakness};

const BASE_ATTACK_DAMAGE: i32 = 12;
const HEAL_AMOUNT: i32 = 15;

#[derive(Debug, Clone, PartialEq)]
pub enum TurnEvent {
    Attacked { line: usize, item: Item, damage: i32, effective: bool },
    Defended { line: usize, item: Item },
    Inspected { line: usize },
    Healed { line: usize, amount: i32 },
    Waited { line: usize },
    BonusStrike { damage: i32 },
    /// RFC-025 regra 1: o monstro golpeia **todo turno**, não só quando o
    /// jogador estoura o orçamento — `truncated` distingue as duas
    /// origens só para o log/storyteller (a matemática de dano já embute
    /// a diferença, ver `TRUNCATE_DAMAGE_MULTIPLIER` em `run_turn_with_bag`).
    CounterAttack { damage: i32, blocked: bool, special: bool, truncated: bool },
    Truncated { line: usize },
    /// `selecionar()` (RFC-015, regra 10): `examined` é quantos itens da
    /// mochila foram varridos até parar (achou ou esgotou a mochila) —
    /// não só o total de ciclos. Sem esse número separado, o jogador não
    /// vê a causa da diferença de custo entre duas ordens de filtro em
    /// `onde:` e conclui que é arbitrário (risco registrado na RFC).
    Selected { line: usize, examined: usize, found: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnResult {
    pub events: Vec<TurnEvent>,
    pub cycles_used: u32,
    pub cycle_budget: u32,
    pub truncated: bool,
    pub player_life: i32,
    pub enemy_life: i32,
}

/// Sinal de controle interno: ou um erro de verdade (aborta sem efeito), ou
/// o orçamento de ciclos estourou (aborta a execução, mas não é um erro).
enum Signal {
    Error(ScriptError),
    Truncated { line: usize },
}

impl From<ScriptError> for Signal {
    fn from(e: ScriptError) -> Self {
        Signal::Error(e)
    }
}

type VResult<T> = Result<T, Signal>;

pub struct Vm<'a> {
    vars: &'a mut HashMap<String, Value>,
    cycles_used: u32,
    cycle_budget: u32,
    dry_run: bool,
    events: Vec<TurnEvent>,

    /// Funções do jogador coletadas antes de rodar o programa (RFC-006,
    /// regra 4): mapa nome -> corpo. Preenchido uma única vez por
    /// `collect_funcs` e clonado para as duas passadas de `run_turn`, o que
    /// garante a regra 13 (coleta idêntica nas duas) por construção — não
    /// por repetir a lógica duas vezes e confiar que elas não divirjam.
    funcs: HashMap<String, Vec<Stmt>>,
    /// Profundidade atual de chamada de função do jogador (RFC-006, regra
    /// 11). Rede de segurança de engenharia: o orçamento de ciclos
    /// (`USER_CALL_COST` por invocação) já trunca a recursão bem antes
    /// disso, já que nenhum monstro tem orçamento >= `MAX_CALL_DEPTH`.
    depth: usize,

    /// Quantas invocações (`StmtKind::Invoke`, RFC-004) já rodaram neste
    /// turno. Verificado contra `api::MAX_INVOCATIONS_PER_TURN` antes de
    /// cada nova invocação — mesmo padrão de `depth`/`MAX_CALL_DEPTH`.
    invocations_this_turn: usize,

    player_life: i32,
    player_max_life: i32,

    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_inspected: bool,

    /// Item usado na última chamada de `defender()` no turno (RFC-016).
    /// `Some` decide o bloqueio binário do contra-ataque (mesmo limiar de
    /// antes, `is_some()` no lugar do `bool`); o item guardado também
    /// alimenta `item_bonus` na hora de reduzir o dano bloqueado. Chamar
    /// `defender()` de novo no mesmo turno sobrescreve — não acumula
    /// (não-objetivo 3 da RFC-016, mesma semântica de "vira `true`" de
    /// antes).
    shielded: Option<Item>,

    /// Equipamento real do jogador (RFC-002, regra 5). `None` reproduz
    /// 100% do comportamento pré-RFC-002 (dano base, sem bônus) — é o que
    /// permite aos 72 testes existentes continuarem passando sem modelar
    /// inventário. `Option<&'a Loadout>` em vez de posse: a VM só lê o
    /// loadout, nunca o modifica (equipar/desequipar é ação do Grimório,
    /// fora do duelo).
    loadout: Option<&'a Loadout>,

    /// Classe do jogador (RFC-003 §1). `None` reproduz 100% do
    /// comportamento pré-RFC-003 (sem bônus de classe) — mesmo espírito de
    /// `loadout: None` na RFC-002, é o que permite os 82 testes existentes
    /// continuarem passando sem escolher classe nenhuma.
    player_class: Option<PlayerClass>,

    /// Mochila real do jogador (RFC-015, regra 6), mesmo padrão de posse
    /// emprestada de `loadout` — a VM só lê, nunca escreve (`selecionar`
    /// não muda o inventário, não-objetivo 4 da RFC). `None` reproduz
    /// "mochila vazia": `selecionar()` sempre devolve `Value::Nil` a custo
    /// zero, nunca erro (regra 9, mesmo espírito de "ausência nunca é
    /// erro" da RFC-002).
    bag: Option<&'a Bag>,

    /// Custo de tamanho (RFC-024) cobrado uma única vez no início de
    /// `exec_program`, guardado à parte de `cycles_used` por um motivo
    /// específico: `Weakness::Eficiencia` (`resolve_attack_by_weakness`)
    /// compara `cycles_used` contra `max_ciclos` em valor **absoluto**, não
    /// contra o orçamento restante — é a única das 7 fraquezas que lê o
    /// contador dessa forma. Sem subtrair este campo, o custo de tamanho
    /// (que mede o *texto escrito*, um eixo que essa fraqueza nunca avaliou)
    /// se somaria à leitura de "quantos ciclos a execução gastou",
    /// corrompendo o limiar de eficiência do Zumbi para qualquer script,
    /// mesmo um curto — não é recalibração, é mudar silenciosamente o que
    /// `Weakness::Eficiencia` mede, o que o não-objetivo 2 da RFC-024
    /// proíbe. `resolve_attack_by_weakness` usa
    /// `cycles_used.saturating_sub(size_charge)` para continuar
    /// comparando só ciclos de execução, exatamente como antes desta RFC.
    size_charge: u32,
}

/// Percorre os `Stmt` de nível superior do programa e registra todo
/// `FuncDef` num mapa nome -> corpo (RFC-006, regra 4) — é o que permite
/// invocar uma função antes de ela aparecer no texto: a coleta roda por
/// completo antes de qualquer instrução ser executada. Só olha o nível
/// superior de propósito: o parser (`parser.rs::parse_func`) já recusa
/// `func` dentro de `if`/`while`/`for`/`func` (regra 5), então um
/// `FuncDef` nunca aparece em outro lugar da árvore.
fn collect_funcs(program: &[Stmt]) -> Result<HashMap<String, Vec<Stmt>>, ScriptError> {
    let mut funcs: HashMap<String, Vec<Stmt>> = HashMap::new();
    for stmt in program {
        if let StmtKind::FuncDef { name, body } = &stmt.kind {
            // nativa primeiro (regra 8) só importa na resolução da chamada;
            // aqui a regra 7 barra de vez colidir com o próprio nome.
            if api::call_cost(name).is_some() {
                return Err(ScriptError::new(
                    stmt.line,
                    format!("'{name}' ja e um ritual conhecido pela Piramide - escolha outro nome pro seu 'func {name}()'"),
                ));
            }
            if funcs.contains_key(name) {
                return Err(ScriptError::new(
                    stmt.line,
                    format!("'{name}' ja ecoa neste script - duas func com o mesmo nome confundem a Piramide, escolha outro"),
                ));
            }
            funcs.insert(name.clone(), body.clone());
        }
    }
    Ok(funcs)
}

/// Conta recursivamente quantos `Stmt` existem na árvore — o "tamanho do
/// texto escrito" que `STMT_SIZE_COST` cobra (RFC-024, regra 2). Desce em
/// todo corpo aninhado (`if`/`while`/`for`/`func`/`invocar`): cada `Stmt`
/// conta uma vez só, não importa quantas vezes o laço/função ao redor dele
/// executa em runtime. Isso é o que faz reusar uma `func` mais barato que
/// reescrever o corpo (regra 3): o corpo de um `FuncDef` só aparece uma vez
/// nesta árvore, na própria definição — cada chamada (`golpe()`) é só a `1`
/// `Stmt` da linha de chamada, o corpo não é contado de novo.
fn count_stmts(stmts: &[Stmt]) -> u32 {
    stmts.iter().map(count_stmt).sum()
}

fn count_stmt(stmt: &Stmt) -> u32 {
    1 + match &stmt.kind {
        StmtKind::Expr(_) | StmtKind::Assign(_, _) => 0,
        StmtKind::If { then_branch, else_branch, .. } => {
            count_stmts(then_branch) + else_branch.as_ref().map(|e| count_stmts(e)).unwrap_or(0)
        }
        StmtKind::While { body, .. } => count_stmts(body),
        StmtKind::For { body, .. } => count_stmts(body),
        StmtKind::FuncDef { body, .. } => count_stmts(body),
        StmtKind::Invoke { body, .. } => count_stmts(body),
    }
}

/// Assinatura pré-RFC-002, preservada byte a byte para não exigir mudança
/// de nenhum dos 72 testes existentes de `script/vm.rs` (RFC-002, regra 5):
/// encaminha para `run_turn_with_loadout` com `loadout: None`, que é
/// exatamente o comportamento anterior — dano base, sem bônus algum.
/// `#[allow(dead_code)]`: fora dos testes (`cfg(test)`), só
/// `run_turn_with_loadout` é chamada (`scenes/duel.rs`) — este crate é um
/// binário, não uma lib, então `pub` não isenta o lint de dead-code do
/// jeito que isentaria numa lib exportada.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn run_turn(
    program: &[Stmt],
    vars: &mut HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_base_damage: i32,
    enemy_special_ready: bool,
) -> Result<TurnResult, ScriptError> {
    run_turn_with_loadout(
        program,
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        enemy_base_damage,
        enemy_special_ready,
        None,
    )
}

/// Mesma execução de turno que `run_turn`, com um `Loadout` real opcional
/// (RFC-002). Assinatura preservada byte a byte pela mesma razão que
/// `run_turn` foi preservada (RFC-003, regra 4): encaminha para
/// `run_turn_with_loadout_and_class` com `player_class: None`, que é
/// exatamente o comportamento anterior a esta RFC — sem bônus de classe.
/// `#[allow(dead_code)]` pelo mesmo motivo de `run_turn`: fora dos testes,
/// só `run_turn_with_loadout_and_class` é chamada (`scenes/duel.rs`).
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn run_turn_with_loadout(
    program: &[Stmt],
    vars: &mut HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_base_damage: i32,
    enemy_special_ready: bool,
    loadout: Option<&Loadout>,
) -> Result<TurnResult, ScriptError> {
    run_turn_with_loadout_and_class(
        program,
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        enemy_base_damage,
        enemy_special_ready,
        loadout,
        None,
    )
}

/// Mesma execução de turno que `run_turn_with_loadout`, com uma
/// `PlayerClass` real opcional (RFC-003 §1). Assinatura preservada byte a
/// byte pela mesma razão das duas anteriores (RFC-015): encaminha para
/// `run_turn_with_bag` com `bag: None`, que é exatamente o comportamento
/// anterior a esta RFC — `selecionar()` sempre devolve `Value::Nil` a
/// custo zero (regra 9). `#[allow(dead_code)]` pelo mesmo motivo das
/// anteriores: fora dos testes, só `run_turn_with_bag` é chamada
/// (`scenes/duel.rs`).
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn run_turn_with_loadout_and_class(
    program: &[Stmt],
    vars: &mut HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_base_damage: i32,
    enemy_special_ready: bool,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
) -> Result<TurnResult, ScriptError> {
    run_turn_with_bag(
        program,
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        enemy_base_damage,
        enemy_special_ready,
        loadout,
        player_class,
        None,
    )
}

/// Mesma execução de turno que `run_turn_with_loadout_and_class`, com uma
/// `Bag` real opcional (RFC-015, regra 6) — é a que `selecionar()` varre.
/// É a única das quatro que a tela de duelo (`scenes/duel.rs`) deveria
/// chamar depois desta RFC — as anteriores continuam existindo só pela
/// compatibilidade dos testes (102 testes existentes, zero assinatura
/// editada).
#[allow(clippy::too_many_arguments)]
pub fn run_turn_with_bag(
    program: &[Stmt],
    vars: &mut HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_base_damage: i32,
    enemy_special_ready: bool,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
) -> Result<TurnResult, ScriptError> {
    // Coleta de funções do jogador (regra 4) uma única vez, antes de
    // qualquer passada. As duas passadas recebem um clone do mesmo mapa —
    // isso satisfaz a regra 13 ("a coleta acontece nas duas, de forma
    // idêntica") por construção, sem duplicar a lógica de coleta.
    let funcs = collect_funcs(program)?;

    // primeira passada: só valida, sem efeitos colaterais — inclusive
    // sobre `vars`. RFC-010 regra 2: opera sobre um *clone* do mapa do
    // jogador, nunca sobre o original, senão uma escrita de variável
    // vazaria do dry-run para o estado real mesmo quando o script trunca
    // antes de chegar lá de verdade na passada de verdade. Reaproveita
    // `probe_pass` (RFC-018) em vez de repetir a lógica aqui.
    probe_pass(
        program,
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        funcs.clone(),
        loadout,
        player_class,
        bag,
    )?;

    // segunda passada: roda de verdade, escrevendo no `vars` emprestado
    // (RFC-010 regra 1/3) — é isso que faz uma variável sobreviver ao
    // turno.
    let mut vm = Vm::new(
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        funcs,
        false,
        loadout,
        player_class,
        bag,
    );
    let truncated = match vm.exec_program(program) {
        Ok(()) => false,
        Err(Signal::Truncated { line }) => {
            vm.events.push(TurnEvent::Truncated { line });
            true
        }
        Err(Signal::Error(e)) => return Err(e),
    };

    // RFC-025 regra 1/2/3: o monstro golpeia **todo turno**, não só quando
    // o jogador estoura o orçamento (causa 1 de
    // `ANALISE-por-que-o-jogo-e-facil.md` — antes desta RFC, toda perda de
    // vida do jogador passava só por aqui, dentro do `if truncated`). A
    // carga decide se esse golpe do turno é o especial (`enemy_special_ready`,
    // já calculado por `MonsterState::special_ready` antes de chamar
    // `run_turn_with_bag` — regra 2). Truncar continua sendo estritamente
    // pior (regra 3): `TRUNCATE_DAMAGE_MULTIPLIER` dobra o dano do turno
    // *antes* de aplicar `defender()`, então bloquear um turno truncado
    // ainda dói mais que não bloquear um turno normal, provado pelo teste
    // `truncating_is_always_strictly_worse_than_not_truncating`.
    let turn_base = if enemy_special_ready { enemy_base_damage * 5 / 2 } else { enemy_base_damage };
    let pre_defense = if truncated { turn_base * api::TRUNCATE_DAMAGE_MULTIPLIER } else { turn_base };
    let blocked = vm.shielded.is_some();
    let dmg = if blocked { pre_defense / 2 } else { pre_defense };
    // RFC-016: reduz o dano bloqueado pelo bônus do item usado em
    // `defender()` (item/classe, mesma fonte de `resolve_attack` e
    // `curar`) — piso em 0, nunca vira cura. Sem bloqueio, ou sem
    // bônus, o comportamento é idêntico ao pré-RFC-016 (regra 4: é isso
    // que faz `defender()` proteger contra o dano do turno, não só o
    // contra-ataque de truncamento).
    let dmg = if blocked {
        let bonus = vm.item_bonus(vm.shielded.as_ref().expect("blocked implica shielded == Some"));
        (dmg - bonus).max(0)
    } else {
        dmg
    };
    vm.player_life = (vm.player_life - dmg).max(0);
    vm.events.push(TurnEvent::CounterAttack { damage: dmg, blocked, special: enemy_special_ready, truncated });

    if !truncated {
        let remaining = cycle_budget.saturating_sub(vm.cycles_used);
        // B-006: golpe bonus so existe pra premiar ciclo sobrando de uma
        // acao real. Script vazio (cycles_used == 0) nao executou nenhuma
        // instrucao - sem esse gate, `remaining` vira o orcamento inteiro e
        // o pior "algoritmo" (nenhum) rivaliza com um bom script.
        if remaining > 0 && vm.cycles_used > 0 && vm.enemy_life > 0 {
            let bonus = remaining as i32;
            vm.enemy_life = (vm.enemy_life - bonus).max(0);
            vm.events.push(TurnEvent::BonusStrike { damage: bonus });
        }
    }

    Ok(TurnResult {
        events: vm.events,
        cycles_used: vm.cycles_used,
        cycle_budget,
        truncated,
        player_life: vm.player_life,
        enemy_life: vm.enemy_life,
    })
}

/// Roda um turno inteiro contra um `MonsterState` de verdade: aplica a
/// progressão real de turno (`begin_turn` — postura alterna, carga soma
/// `CHARGE_PER_TURN`) **antes** de chamar `run_turn_with_bag`, e escreve o
/// resultado de volta no próprio `MonsterState` (`life`, `consume_charge`
/// se o golpe revelado foi o especial) **depois**. É a mesma sequência que
/// `scenes/duel.rs::run_script` fazia inline para o turno real; extraída
/// aqui (RFC-027) para ser a ÚNICA rotina de avanço de turno, chamada tanto
/// pelo turno real (`monster: &mut MonsterState` emprestado da cena) quanto
/// pelo Ensaio (`script::rehearsal::rehearse`, sobre um clone local) — nunca
/// duas cópias da lógica de "o que acontece entre dois turnos".
#[allow(clippy::too_many_arguments)]
pub fn simulate_turn(
    program: &[Stmt],
    vars: &mut HashMap<String, Value>,
    monster: &mut crate::monsters::MonsterState,
    player_life: i32,
    player_max_life: i32,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
) -> Result<TurnResult, ScriptError> {
    monster.begin_turn();
    let special_ready = monster.special_ready();
    let result = run_turn_with_bag(
        program,
        vars,
        monster.spec.cycle_budget,
        player_life,
        player_max_life,
        monster.life,
        monster.spec.max_life,
        monster.posture,
        monster.spec.weakness,
        monster.spec.base_damage,
        special_ready,
        loadout,
        player_class,
        bag,
    )?;
    monster.life = result.enemy_life;
    if result.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { special: true, .. })) {
        monster.consume_charge();
    }
    Ok(result)
}

/// Resultado de uma passada de **validação apenas** (RFC-018): nenhum
/// efeito colateral real (vida, contra-ataque, golpe bônus, `vars` do
/// chamador) — só o que a barra de ciclos da tela de duelo precisa pra
/// mostrar um número honesto antes do jogador apertar EXECUTAR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeResult {
    pub cycles_used: u32,
    /// Estourar o orçamento durante a validação ao vivo não é erro (RFC-018
    /// regra 4) — só informa pra a UI colorir a barra de alerta, do mesmo
    /// jeito que `over` já faz hoje.
    pub truncated: bool,
}

/// Passada de validação compartilhada entre `run_turn_with_bag` (que a usa
/// como primeira das suas duas passadas) e `probe_turn_with_bag` (RFC-018,
/// que expõe só isso pra validação ao vivo). Sempre opera sobre um *clone*
/// de `vars` — nunca sobre o mapa do chamador (mesma disciplina da
/// RFC-010): mesmo quando o chamador só tem um clone descartável, clonar
/// de novo aqui mantém uma única lógica de dry-run em vez de duas cópias
/// divergentes.
#[allow(clippy::too_many_arguments)]
fn probe_pass(
    program: &[Stmt],
    vars: &HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    funcs: HashMap<String, Vec<Stmt>>,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
) -> Result<ProbeResult, ScriptError> {
    let mut probe_vars = vars.clone();
    let mut probe = Vm::new(
        &mut probe_vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        funcs,
        true,
        loadout,
        player_class,
        bag,
    );
    match probe.exec_program(program) {
        Ok(()) => Ok(ProbeResult { cycles_used: probe.cycles_used, truncated: false }),
        Err(Signal::Truncated { .. }) => Ok(ProbeResult { cycles_used: probe.cycles_used, truncated: true }),
        Err(Signal::Error(e)) => Err(e),
    }
}

/// Validação ao vivo (RFC-018): mesma passada `dry_run` que
/// `run_turn_with_bag` já roda internamente como primeira passada, exposta
/// aqui sozinha — sem a segunda passada real — pra tela de duelo poder
/// mostrar sintaxe/ciclos honestos a cada frame sem pagar o custo de rodar
/// a VM duas vezes a mais (a real e a dry-run dela) por chamada. `vars` é
/// só lido: o clone interno de `probe_pass` garante que o mapa do jogador
/// nunca é mutado por uma validação ao vivo, não importa quantas rodem
/// sem o jogador apertar EXECUTAR.
#[allow(clippy::too_many_arguments)]
pub fn probe_turn_with_bag(
    program: &[Stmt],
    vars: &HashMap<String, Value>,
    cycle_budget: u32,
    player_life: i32,
    player_max_life: i32,
    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
) -> Result<ProbeResult, ScriptError> {
    let funcs = collect_funcs(program)?;
    probe_pass(
        program,
        vars,
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        funcs,
        loadout,
        player_class,
        bag,
    )
}

impl<'a> Vm<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        vars: &'a mut HashMap<String, Value>,
        cycle_budget: u32,
        player_life: i32,
        player_max_life: i32,
        enemy_life: i32,
        enemy_max_life: i32,
        enemy_posture: Posture,
        enemy_weakness: Weakness,
        funcs: HashMap<String, Vec<Stmt>>,
        dry_run: bool,
        loadout: Option<&'a Loadout>,
        player_class: Option<PlayerClass>,
        bag: Option<&'a Bag>,
    ) -> Self {
        Vm {
            vars,
            cycles_used: 0,
            cycle_budget,
            dry_run,
            events: Vec::new(),
            funcs,
            depth: 0,
            invocations_this_turn: 0,
            player_life,
            player_max_life,
            enemy_life,
            enemy_max_life,
            enemy_posture,
            enemy_weakness,
            enemy_inspected: false,
            shielded: None,
            loadout,
            player_class,
            bag,
            size_charge: 0,
        }
    }

    fn err(&self, line: usize, message: impl Into<String>) -> Signal {
        Signal::Error(ScriptError::new(line, message))
    }

    fn charge(&mut self, line: usize, cost: u32) -> VResult<()> {
        if self.cycles_used + cost > self.cycle_budget {
            return Err(Signal::Truncated { line });
        }
        self.cycles_used += cost;
        Ok(())
    }

    fn exec_program(&mut self, program: &[Stmt]) -> VResult<()> {
        // RFC-024 regra 2/4: custo de tamanho cobrado uma única vez, antes
        // de qualquer instrução executar, contra o mesmo orçamento do turno
        // — não é um segundo recurso. `exec_program` é o único ponto de
        // entrada que tanto `run_turn_with_bag` (segunda passada, real)
        // quanto `probe_pass`/`probe_turn_with_bag` (RFC-018, validação ao
        // vivo) atravessam, então cobrar aqui satisfaz a regra 5 de graça —
        // sem duplicar a cobrança nos dois lugares. Estourar aqui é
        // `Signal::Truncated` normal (contra-ataque incluso), igual a
        // qualquer outro `charge`. Programa vazio conta 0 `Stmt`, custa 0 —
        // usa a linha 1 como âncora só para o relato de truncamento (nunca
        // dispara nesse caso, custo é 0).
        let size = count_stmts(program);
        let line = program.first().map(|s| s.line).unwrap_or(1);
        let cost = size * api::STMT_SIZE_COST;
        self.charge(line, cost)?;
        self.size_charge = cost;
        self.exec_block(program)
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> VResult<()> {
        for s in stmts {
            self.exec_stmt(s)?;
        }
        Ok(())
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> VResult<()> {
        let line = stmt.line;
        match &stmt.kind {
            StmtKind::Expr(e) => {
                self.eval(e, line)?;
                Ok(())
            }
            StmtKind::Assign(name, expr) => {
                let v = self.eval(expr, line)?;
                self.vars.insert(name.clone(), v);
                Ok(())
            }
            StmtKind::If { cond, then_branch, else_branch } => {
                self.charge(line, BRANCH_COST)?;
                let c = self.eval(cond, line)?;
                if c.as_bool() {
                    self.exec_block(then_branch)?;
                } else if let Some(e) = else_branch {
                    self.exec_block(e)?;
                }
                Ok(())
            }
            StmtKind::While { cond, body } => {
                loop {
                    self.charge(line, LOOP_TICK_COST)?;
                    let c = self.eval(cond, line)?;
                    if !c.as_bool() {
                        break;
                    }
                    self.exec_block(body)?;
                }
                Ok(())
            }
            StmtKind::For { var, from, to, body } => {
                let from_v = self
                    .eval(from, line)?
                    .as_num()
                    .ok_or_else(|| self.err(line, "inicio do intervalo precisa ser numero"))?;
                let to_v = self
                    .eval(to, line)?
                    .as_num()
                    .ok_or_else(|| self.err(line, "fim do intervalo precisa ser numero"))?;
                let mut i = from_v as i64;
                let end = to_v as i64;
                while i < end {
                    self.charge(line, LOOP_TICK_COST)?;
                    self.vars.insert(var.clone(), Value::Num(i as f64));
                    self.exec_block(body)?;
                    i += 1;
                }
                Ok(())
            }
            // Declarar uma função não é executá-la (regra 3): nenhum ciclo
            // é cobrado aqui. A coleta que torna a função invocável já
            // aconteceu em `collect_funcs`, antes do programa começar a
            // rodar — este statement não faz mais nada.
            StmtKind::FuncDef { .. } => Ok(()),
            // `invocar nome:` (RFC-004) — os 5 passos da decisão de
            // arquitetura, na ordem exata da RFC. `name` é só rótulo
            // narrativo/de log (regra 1); não afeta a execução.
            StmtKind::Invoke { name: _, body } => self.exec_invoke(line, body),
        }
    }

    /// Passos 1-5 da "Decisão de arquitetura" da RFC-004. Nenhuma
    /// suspensão: `exec_block` roda até o fim exatamente como já fazia,
    /// só que com `(cycle_budget, cycles_used)` trocados temporariamente
    /// por um sub-orçamento fixo — o mesmo padrão de "salvar/trocar/
    /// restaurar" que `eval_user_call` já usa para `self.depth`.
    fn exec_invoke(&mut self, line: usize, body: &[Stmt]) -> VResult<()> {
        // 1. Cobra INVOKE_COST do orçamento principal. Estourar aqui é
        // estourar o orçamento principal — `charge` já devolve
        // `Signal::Truncated`, que propaga normal (contra-ataque incluso).
        self.charge(line, api::INVOKE_COST)?;

        // 2. Limite de invocações por turno, mesmo padrão de MAX_CALL_DEPTH.
        if self.invocations_this_turn >= api::MAX_INVOCATIONS_PER_TURN {
            return Err(self.err(
                line,
                format!(
                    "limite de invocacoes por turno excedido (maximo {})",
                    api::MAX_INVOCATIONS_PER_TURN
                ),
            ));
        }
        self.invocations_this_turn += 1;

        // 3. Salva/troca o contador de ciclos por um sub-orcamento isolado.
        let outer_budget = self.cycle_budget;
        let outer_used = self.cycles_used;
        self.cycle_budget = api::INVOKE_BUDGET;
        self.cycles_used = 0;

        let result = self.exec_block(body);

        // 5. Restaura o contador principal - ciclos gastos dentro da
        // invocacao nunca contam contra o orcamento principal nem sobram
        // para o bonus do turno (nao-objetivo 5 da RFC).
        self.cycle_budget = outer_budget;
        self.cycles_used = outer_used;

        // 4. Truncamento interno nao propaga (nao e o turno que estourou);
        // erro real propaga normalmente.
        match result {
            Ok(()) => Ok(()),
            Err(Signal::Truncated { .. }) => Ok(()),
            Err(Signal::Error(e)) => Err(Signal::Error(e)),
        }
    }

    fn eval(&mut self, expr: &Expr, line: usize) -> VResult<Value> {
        match expr {
            Expr::Number(n) => Ok(Value::Num(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Ident(name) => self.eval_ident(name, line),
            Expr::Index(base, key) => {
                let base_v = self.eval(base, line)?;
                let key_v = self.eval(key, line)?;
                match base_v {
                    Value::Collection(kind) => {
                        let name = key_v
                            .as_str()
                            .ok_or_else(|| self.err(line, "indice precisa ser texto, ex.: magia[\"fogo\"]"))?
                            .to_string();
                        Ok(Value::Item(Item { kind, name, bonus_damage: 0 }))
                    }
                    other => Err(self.err(line, format!("nao e possivel indexar um {}", other.type_name()))),
                }
            }
            Expr::Field(base, field) => {
                let base_v = self.eval(base, line)?;
                match base_v {
                    Value::EntityRef(target) => self.eval_field(target, field, line),
                    // acesso "por enum": magia.Fogo == magia["fogo"] — mesma
                    // coleção, só sem aspas; o nome vira minúsculo pra bater
                    // com o esquema de nomes usado em Element::from_name.
                    Value::Collection(kind) => Ok(Value::Item(Item { kind, name: field.to_lowercase(), bonus_damage: 0 })),
                    // RFC-015 regra 5: `item.nome`/`item.tipo`/`item.bonus` —
                    // é o que `onde:` de `selecionar()` consulta. Não tem
                    // relação com `resolve_attack` (não-objetivo 5): é só
                    // leitura do valor, nunca um segundo caminho de bônus.
                    Value::Item(item) => self.eval_item_field(&item, field, line),
                    other => Err(self.err(line, format!("nao e possivel acessar campo em um {}", other.type_name()))),
                }
            }
            Expr::Call(name, args) => self.eval_call(name, args, line),
            Expr::Unary(op, e) => {
                let v = self.eval(e, line)?;
                match op {
                    UnaryOp::Neg => {
                        let n = v.as_num().ok_or_else(|| self.err(line, "esperava numero depois de '-'"))?;
                        Ok(Value::Num(-n))
                    }
                    UnaryOp::Not => Ok(Value::Bool(!v.as_bool())),
                }
            }
            Expr::Binary(l, op, r) => self.eval_binary(l, *op, r, line),
            Expr::Select { predicate, limit } => self.eval_select(predicate, limit, line),
        }
    }

    /// `selecionar(mochila, onde: <predicate>, limite: <limit>)` (RFC-015,
    /// regras 3, 6-9). `limite` só aceita o literal `1` — qualquer outro
    /// valor é erro claro nesta linha, nunca truncamento silencioso
    /// (não-objetivo 2 da RFC). Sem `bag` ou mochila vazia: `Value::Nil` a
    /// custo zero, sem passar pelo laço (regra 9). Cada item examinado
    /// cobra `SELECT_SCAN_COST`, liga `vars["item"]` temporariamente
    /// (salvando/restaurando o valor anterior, se houver) e avalia
    /// `predicate` — o curto-circuito de `and`/`or` já existente em
    /// `eval_binary` decide quantas cláusulas rodar *dentro* de cada item;
    /// esta função decide só quantos *itens* são examinados até o
    /// primeiro match (regra 8).
    fn eval_select(&mut self, predicate: &Expr, limit: &Expr, line: usize) -> VResult<Value> {
        let limit_v = self.eval(limit, line)?;
        let limit_n = limit_v
            .as_num()
            .ok_or_else(|| self.err(line, format!("'limite' precisa ser numero, encontrei {}", limit_v.type_name())))?;
        if limit_n != 1.0 {
            return Err(self.err(line, format!("'selecionar' so aceita limite: 1 nesta versao da Piramide, encontrei limite: {limit_n}")));
        }

        // Extrai os dados da mochila antes do laço (não a referência) para
        // não manter `self.bag` emprestado enquanto o resto do método
        // precisa de `&mut self` para cobrar ciclo e avaliar o predicado.
        let entries: Vec<(ItemKind, String, i32)> =
            self.bag.map(|bag| bag.0.iter().map(|(item, _qty)| (item.kind, item.name.clone(), item.bonus_damage)).collect()).unwrap_or_default();

        let previous_item = self.vars.get("item").cloned();
        let mut examined = 0usize;
        let mut found: Option<Item> = None;

        for (kind, name, bonus_damage) in entries {
            self.charge(line, api::SELECT_SCAN_COST)?;
            examined += 1;

            let candidate = Item { kind, name, bonus_damage };
            self.vars.insert("item".to_string(), Value::Item(candidate.clone()));
            let matched = self.eval(predicate, line)?.as_bool();
            if matched {
                found = Some(candidate);
                break;
            }
        }

        match previous_item {
            Some(v) => {
                self.vars.insert("item".to_string(), v);
            }
            None => {
                self.vars.remove("item");
            }
        }

        if !self.dry_run {
            self.events.push(TurnEvent::Selected { line, examined, found: found.is_some() });
        }

        Ok(match found {
            Some(item) => Value::Item(item),
            None => Value::Nil,
        })
    }

    fn eval_ident(&mut self, name: &str, line: usize) -> VResult<Value> {
        if name == "eu" {
            return Ok(Value::EntityRef(Target::Me));
        }
        if name == "inimigo" {
            return Ok(Value::EntityRef(Target::Enemy));
        }
        if let Some(kind) = ItemKind::from_ident(name) {
            return Ok(Value::Collection(kind));
        }
        if let Some(v) = self.vars.get(name) {
            return Ok(v.clone());
        }
        Err(self.err(line, format!("variavel nao definida: '{name}'")))
    }

    fn eval_field(&mut self, target: Target, field: &str, line: usize) -> VResult<Value> {
        let (life, max_life) = match target {
            Target::Me => (self.player_life, self.player_max_life),
            Target::Enemy => (self.enemy_life, self.enemy_max_life),
        };
        match field {
            "vida" => Ok(Value::Num(life as f64)),
            "vida_max" => Ok(Value::Num(max_life as f64)),
            "postura" if target == Target::Enemy => Ok(Value::Str(self.enemy_posture.as_str().to_string())),
            "ciclos" => Ok(Value::Num((self.cycle_budget.saturating_sub(self.cycles_used)) as f64)),
            other => Err(self.err(line, format!("campo desconhecido: '{other}'"))),
        }
    }

    /// Campos de um `Value::Item` (RFC-015, regra 5): `.nome`, `.tipo`
    /// (via `ItemKind::label()`) e `.bonus` (`bonus_damage`, RFC-015 regra
    /// 4). É o que permite `onde:` de `selecionar()` consultar o item sob
    /// exame sem depender de nenhum campo novo em `Vm`.
    fn eval_item_field(&self, item: &Item, field: &str, line: usize) -> VResult<Value> {
        match field {
            "nome" => Ok(Value::Str(item.name.clone())),
            "tipo" => Ok(Value::Str(item.kind.label().to_string())),
            "bonus" => Ok(Value::Num(item.bonus_damage as f64)),
            other => Err(self.err(line, format!("campo desconhecido em item: '{other}'"))),
        }
    }

    fn eval_binary(&mut self, l: &Expr, op: BinOp, r: &Expr, line: usize) -> VResult<Value> {
        // curto-circuito para 'and'/'or'
        if op == BinOp::And {
            let lv = self.eval(l, line)?;
            if !lv.as_bool() {
                return Ok(Value::Bool(false));
            }
            let rv = self.eval(r, line)?;
            return Ok(Value::Bool(rv.as_bool()));
        }
        if op == BinOp::Or {
            let lv = self.eval(l, line)?;
            if lv.as_bool() {
                return Ok(Value::Bool(true));
            }
            let rv = self.eval(r, line)?;
            return Ok(Value::Bool(rv.as_bool()));
        }

        let lv = self.eval(l, line)?;
        let rv = self.eval(r, line)?;

        match op {
            BinOp::Eq => return Ok(Value::Bool(values_equal(&lv, &rv))),
            BinOp::NotEq => return Ok(Value::Bool(!values_equal(&lv, &rv))),
            _ => {}
        }

        let ln = lv.as_num().ok_or_else(|| self.err(line, format!("esperava numero, encontrei {}", lv.type_name())))?;
        let rn = rv.as_num().ok_or_else(|| self.err(line, format!("esperava numero, encontrei {}", rv.type_name())))?;

        Ok(match op {
            BinOp::Add => Value::Num(ln + rn),
            BinOp::Sub => Value::Num(ln - rn),
            BinOp::Mul => Value::Num(ln * rn),
            BinOp::Div => {
                if rn == 0.0 {
                    return Err(self.err(line, "divisao por zero"));
                }
                Value::Num(ln / rn)
            }
            BinOp::Mod => Value::Num(ln % rn),
            BinOp::Lt => Value::Bool(ln < rn),
            BinOp::Gt => Value::Bool(ln > rn),
            BinOp::Le => Value::Bool(ln <= rn),
            BinOp::Ge => Value::Bool(ln >= rn),
            BinOp::Eq | BinOp::NotEq | BinOp::And | BinOp::Or => unreachable!(),
        })
    }

    /// Resolução de chamada (RFC-006, regra 8): nativa primeiro, depois
    /// função do jogador, só então erro. Nativa primeiro é o que garante
    /// que a regra 7 (nome de função não pode colidir com nativa) nunca é
    /// contornável — `collect_funcs` já barra a colisão na coleta, mas essa
    /// ordem aqui é a segunda trava, independente da primeira.
    fn eval_call(&mut self, name: &str, args: &[Expr], line: usize) -> VResult<Value> {
        if let Some(cost) = api::call_cost(name) {
            return self.eval_native_call(name, cost, args, line);
        }
        if let Some(body) = self.funcs.get(name).cloned() {
            return self.eval_user_call(name, &body, args, line);
        }
        Err(self.err(line, format!("funcao desconhecida: '{name}'")))
    }

    /// Invocação de função do jogador (RFC-006, regras 9, 10, 11, 12).
    /// `USER_CALL_COST` é cobrado antes do corpo e soma ao custo do corpo
    /// (regra 9) — é isso que faz recursão infinita truncar pelo orçamento
    /// em vez de estourar a pilha do Rust. `MAX_CALL_DEPTH` é a rede de
    /// segurança de engenharia por trás disso (regra 11); estourar o
    /// orçamento dentro do corpo se comporta exatamente como estourar fora
    /// dele (regra 12), porque `charge`/`Signal::Truncated` não fazem
    /// distinção — não há caso especial aqui.
    fn eval_user_call(&mut self, name: &str, body: &[Stmt], args: &[Expr], line: usize) -> VResult<Value> {
        self.charge(line, api::USER_CALL_COST)?;

        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.eval(a, line)?);
        }
        if !values.is_empty() {
            return Err(self.err(line, format!("funcao '{name}' nao recebe argumento, mas foi chamada com {}", values.len())));
        }

        self.depth += 1;
        if self.depth > api::MAX_CALL_DEPTH {
            self.depth -= 1;
            return Err(self.err(
                line,
                format!("'{name}': profundidade maxima de chamada excedida (limite {})", api::MAX_CALL_DEPTH),
            ));
        }
        let result = self.exec_block(body);
        self.depth -= 1;
        result?;
        Ok(Value::Nil)
    }

    fn eval_native_call(&mut self, name: &str, cost: u32, args: &[Expr], line: usize) -> VResult<Value> {
        self.charge(line, cost)?;

        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.eval(a, line)?);
        }

        match name {
            "atacar" => {
                let item = self.expect_item(&values, name, line)?;
                if !self.dry_run {
                    let (damage, effective) = self.resolve_attack(&item);
                    self.enemy_life = (self.enemy_life - damage).max(0);
                    self.events.push(TurnEvent::Attacked { line, item, damage, effective });
                }
                Ok(Value::Nil)
            }
            "defender" => {
                let item = self.expect_item(&values, name, line)?;
                if !self.dry_run {
                    self.shielded = Some(item.clone());
                    self.events.push(TurnEvent::Defended { line, item });
                }
                Ok(Value::Nil)
            }
            "inspecionar" => {
                self.expect_arity(&values, 0, name, line)?;
                if !self.dry_run {
                    self.enemy_inspected = true;
                    self.events.push(TurnEvent::Inspected { line });
                }
                Ok(Value::Nil)
            }
            "curar" => {
                let item = self.expect_item(&values, name, line)?;
                if !self.dry_run {
                    let amount = HEAL_AMOUNT + self.item_bonus(&item);
                    self.player_life = (self.player_life + amount).min(self.player_max_life);
                    self.events.push(TurnEvent::Healed { line, amount });
                }
                Ok(Value::Nil)
            }
            "esperar" => {
                self.expect_arity(&values, 0, name, line)?;
                if !self.dry_run {
                    self.events.push(TurnEvent::Waited { line });
                }
                Ok(Value::Nil)
            }
            // inalcançável: só chegamos aqui quando `api::call_cost(name)`
            // já devolveu `Some`, e ela só devolve `Some` para essas cinco.
            _ => unreachable!("nome nativo sem ramo em eval_native_call: '{name}'"),
        }
    }

    fn expect_arity(&self, values: &[Value], n: usize, name: &str, line: usize) -> VResult<()> {
        if values.len() != n {
            return Err(self.err(line, format!("'{name}' espera {n} argumento(s), recebeu {}", values.len())));
        }
        Ok(())
    }

    fn expect_item(&self, values: &[Value], name: &str, line: usize) -> VResult<Item> {
        self.expect_arity(values, 1, name, line)?;
        match &values[0] {
            Value::Item(item) => Ok(item.clone()),
            other => Err(self.err(line, format!("'{name}' espera um item (ex.: espada[\"fogo\"]), recebeu {}", other.type_name()))),
        }
    }

    /// Bônus do item equipado (RFC-002, regra 6): soma-se depois da
    /// decisão de fraqueza (não muda `effective`, nem a lógica de nenhum
    /// dos 6 braços de `Weakness` abaixo — não-objetivo 4 da RFC). Sem
    /// `loadout`, ou sem correspondência de `kind`+`name`
    /// (case-insensitive, mesmo padrão de `to_lowercase()` já usado no
    /// acesso por enum), soma zero: idêntico ao comportamento anterior à
    /// RFC-002. Item ausente nunca é erro — só ausência de bônus.
    fn equipped_bonus(&self, item: &Item) -> i32 {
        self.loadout
            .and_then(|lo| lo.slot(item.kind))
            .filter(|equipped| equipped.name.to_lowercase() == item.name.to_lowercase())
            .map(|equipped| equipped.bonus_damage)
            .unwrap_or(0)
    }

    /// Bônus de classe (RFC-003 §1): soma depois do bônus de item
    /// equipado, sem alterar `effective` nem a decisão de fraqueza — mesmo
    /// padrão aditivo de `equipped_bonus`. Vale independentemente de o
    /// item estar equipado no `Loadout` (regra 5 da RFC-003: a afinidade é
    /// sobre o *tipo* de arma usada no ataque, não sobre posse).
    fn class_bonus(&self, item: &Item) -> i32 {
        match self.player_class {
            Some(class) if class.affinity() == item.kind => api::CLASS_BONUS_DAMAGE,
            _ => 0,
        }
    }

    /// Soma de `equipped_bonus` + `class_bonus` (RFC-014): ponto único de
    /// correspondência de slot/kind/afinidade, reaproveitado por
    /// `resolve_attack` e por `curar()` — evita duas fontes de verdade
    /// para a mesma regra aditiva.
    fn item_bonus(&self, item: &Item) -> i32 {
        self.equipped_bonus(item) + self.class_bonus(item)
    }

    fn resolve_attack(&self, item: &Item) -> (i32, bool) {
        let (base, effective) = self.resolve_attack_by_weakness(item);
        (base + self.item_bonus(item), effective)
    }

    // RFC-021: das 7 fraquezas, só `RequerInspecao` bloqueava dano por
    // completo -- as outras 6 reduziam mas nunca bloqueavam, e o usuário
    // decidiu que "reduz" precisava doer bem mais sem virar bloqueio total
    // (meio-termo, não uniformizar tudo pra 0). `DuploSelo` já usava `/8`
    // desde a RFC-011; esta RFC estende o mesmo divisor (não um valor novo
    // chutado) às outras 5 reduções (`Elemento`, `Eficiencia`, `ExigeGuarda`,
    // `ExigeNomeacao`, `ExigeInvocacaoDupla`), calibradas de novo pela
    // bateria de testes de ordenação logo abaixo (`mod tests`).
    fn resolve_attack_by_weakness(&self, item: &Item) -> (i32, bool) {
        match self.enemy_weakness {
            Weakness::Elemento(elem) => {
                if Element::from_name(&item.name) == elem {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
            Weakness::Eficiencia { max_ciclos } => {
                // RFC-024: subtrai o custo de tamanho antes de comparar --
                // ver o doc comment de `size_charge` no struct `Vm`. Este
                // limiar sempre mediu ciclos de *execução*, nunca o tamanho
                // do script escrito.
                if self.cycles_used.saturating_sub(self.size_charge) <= max_ciclos {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
            // RFC-021: ao contrário das outras 5 fraquezas reduzidas, a
            // condição de `ExigeGuarda` é *ambiente* (a postura alterna
            // sozinha a cada turno, `Posture::toggled`) em vez de exigir uma
            // ação do jogador -- um script que nunca lê `inimigo.postura`
            // ainda acerta dano cheio em ~metade dos turnos, de graça, só
            // por sorte de postura. Isso limita estruturalmente o quanto
            // qualquer divisor consegue punir (ver
            // `beetle_naive_spam_never_beats_posture_branch` e a nota do
            // gamedev na entrega desta RFC) -- `/8` ainda é o divisor certo
            // (mesmo piso `> 0` das outras), só que a margem prática exige
            // recalibrar vida/orçamento do Escaravelho (`data.rs::beetle`),
            // não o divisor em si.
            Weakness::ExigeGuarda => {
                if self.enemy_posture == Posture::Guarda {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
            Weakness::RequerInspecao => {
                if self.enemy_inspected {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (0, false)
                }
            }
            // RFC-008 regra 1: as duas condições já ensinadas separadamente
            // (postura em guarda, inspecionado neste turno) precisam valer
            // ao mesmo tempo — nenhuma isolada basta. `enemy_inspected` já
            // existe e persiste dentro do turno (regra 7): não precisa de
            // campo novo em `Vm` nem de variável de script para "lembrar".
            // RFC-011: `/4` permitia que um spam ingenuo de `atacar()` (sem
            // compor postura+inspecao) vencesse Aker mais rapido que o
            // script correto -- antijogo real achado pelo QA. `/8` restaura
            // a margem: a reducao so se aplica quando as duas condicoes nao
            // valem juntas, e agora e punitiva o suficiente para isso.
            Weakness::DuploSelo => {
                if self.enemy_posture == Posture::Guarda && self.enemy_inspected {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
            // RFC-012: julga a *forma* do script, não o estado do combate.
            // `self.depth` (RFC-006) sobe antes do corpo de uma `func`
            // rodar e desce depois (`eval_user_call`) -- lido aqui sem
            // campo novo, exatamente como a regra 1 da RFC pede. Divisor
            // recalibrado pela RFC-021 (`/4` -> `/8`, mesmo valor das
            // outras reduções agora) e reverificado pelo teste de ordenação
            // `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns`:
            // a estrategia ingenua (atacar() repetido no nivel superior)
            // continua perdendo em turnos com margem clara contra a
            // estrategia correta (mesmo atacar(), de dentro de uma func).
            Weakness::ExigeNomeacao => {
                if self.depth > 0 {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
            // RFC-017: le `self.invocations_this_turn` (RFC-004, campo ja
            // existente, incrementado em `exec_invoke`) sem estado novo.
            // Nao importa se o atacar() decisivo veio de dentro ou de fora
            // de uma invocacao (nao-objetivo 4) -- so que 2 ja tenham
            // rodado no turno. Divisor recalibrado pela RFC-021 (`/4` ->
            // `/8`) e reverificado pelo teste de ordenacao
            // `exige_invocacao_dupla_beats_naive_spam_in_fewer_turns`: o
            // spam ingenuo (atacar() repetido, sem invocar) continua
            // perdendo com margem clara contra a estrategia correta (2
            // invocacoes + atacar()), mesma disciplina da RFC-011/012.
            Weakness::ExigeInvocacaoDupla => {
                if self.invocations_this_turn >= api::MAX_INVOCATIONS_PER_TURN {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 8, false)
                }
            }
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Item as InvItem;
    use crate::monsters::{data, MonsterSpec};
    use crate::script::parser::parse;

    fn run(src: &str, budget: u32, weakness: Weakness, posture: Posture) -> TurnResult {
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn(&program, &mut vars, budget, 100, 100, 100, 100, posture, weakness, 10, false).unwrap()
    }

    #[test]
    fn correct_element_deals_full_damage() {
        let r = run("atacar(magia[\"fogo\"])\n", 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert!(r.enemy_life < 100 - 5); // dano cheio (12) bem maior que dano reduzido (4)
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn dot_access_is_equivalent_to_bracket_string() {
        // magia.Fogo (sem aspas) precisa se comportar exatamente como
        // magia["fogo"] — mesmo item, mesmo dano.
        let bracket = run("atacar(magia[\"fogo\"])\n", 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        let dotted = run("atacar(magia.Fogo)\n", 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert_eq!(bracket.enemy_life, dotted.enemy_life);
    }

    #[test]
    fn wrong_element_deals_reduced_damage() {
        let r = run("atacar(magia[\"agua\"])\n", 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8); // RFC-021: /3 -> /8
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn posture_branch_picks_right_action() {
        // if na postura: se guarda, ataca; script correto contra ExigeGuarda
        let src = "if inimigo.postura == \"guarda\":\n    atacar(espada[\"ferro\"])\nelse:\n    esperar()\n";
        let r = run(src, 20, Weakness::ExigeGuarda, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, .. } => assert!(*effective),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn budget_overflow_truncates_and_counterattacks() {
        // while inimigo.vida > 0: atacar() -- nunca termina sozinho, deve estourar
        let src = "while inimigo.vida > 0:\n    atacar(espada[\"ferro\"])\n";
        let r = run(src, 6, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert!(r.truncated);
        assert!(r.player_life < 100);
        assert!(matches!(r.events.last(), Some(TurnEvent::CounterAttack { .. })));
    }

    #[test]
    fn special_ready_makes_counterattack_bigger() {
        let src = "while inimigo.vida > 0:\n    atacar(espada[\"ferro\"])\n";
        let program = parse(src).unwrap();
        let normal = run_turn(&program, &mut HashMap::new(), 6, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        let special = run_turn(&program, &mut HashMap::new(), 6, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, true).unwrap();
        let dmg = |r: &TurnResult| match r.events.last() {
            Some(TurnEvent::CounterAttack { damage, special, .. }) => (*damage, *special),
            other => panic!("evento inesperado: {other:?}"),
        };
        let (normal_dmg, normal_special) = dmg(&normal);
        let (special_dmg, special_special) = dmg(&special);
        assert!(!normal_special);
        assert!(special_special);
        assert!(special_dmg > normal_dmg);
    }

    #[test]
    fn efficient_script_gets_bonus_damage() {
        let r = run("atacar(magia[\"fogo\"])\n", 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert!(!r.truncated);
        assert!(matches!(r.events.last(), Some(TurnEvent::BonusStrike { .. })));
        // RFC-025 regra 1: o monstro ataca todo turno agora, mesmo sem
        // truncar -- vida cheia deixou de ser garantida so por nao errar.
        // `run()` usa enemy_base_damage=10, sem defender(), sem carga
        // cheia: dano do turno = 10 cheio.
        assert_eq!(r.player_life, 100 - 10);
    }

    #[test]
    fn empty_script_gets_no_bonus_strike() {
        // B-006: um script vazio (0 instrucoes, 0 ciclos gastos) nao pode
        // receber BonusStrike. Sem o gate `cycles_used > 0` em `run_turn`,
        // `remaining` seria igual ao `cycle_budget` inteiro e o inimigo
        // levaria dano flat sem nenhuma acao real do jogador.
        let r = run("", 10, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert_eq!(r.cycles_used, 0);
        assert!(!r.truncated);
        assert_eq!(r.enemy_life, 100, "script vazio nao pode causar dano nenhum");
        assert!(
            !r.events.iter().any(|e| matches!(e, TurnEvent::BonusStrike { .. })),
            "script vazio nao pode gerar BonusStrike: {:?}",
            r.events
        );
    }

    #[test]
    fn quadratic_loop_costs_more_than_linear() {
        let linear = "for i in 0..5:\n    esperar()\n";
        let quadratic = "for i in 0..5:\n    for j in 0..5:\n        esperar()\n";
        let rl = run(linear, 100, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        let rq = run(quadratic, 100, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert!(rq.cycles_used > rl.cycles_used);
    }

    #[test]
    fn syntax_error_does_not_touch_state() {
        let program_err = parse("atacar(\n");
        assert!(program_err.is_err());
    }

    #[test]
    fn runtime_error_does_not_consume_turn() {
        let program = parse("atacar(naoexiste)\n").unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false);
        assert!(err.is_err());
    }

    #[test]
    fn inspect_required_before_damage_lands() {
        let blind = run("atacar(espada[\"ferro\"])\n", 20, Weakness::RequerInspecao, Posture::Guarda);
        match &blind.events[0] {
            TurnEvent::Attacked { damage, .. } => assert_eq!(*damage, 0),
            other => panic!("evento inesperado: {other:?}"),
        }

        let src = "inspecionar()\natacar(espada[\"ferro\"])\n";
        let seen = run(src, 20, Weakness::RequerInspecao, Posture::Guarda);
        match &seen.events[1] {
            TurnEvent::Attacked { damage, .. } => assert_eq!(*damage, BASE_ATTACK_DAMAGE),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    // --- RFC-006: funções definidas pelo jogador ---------------------

    /// O teste mais importante desta RFC. A premissa original da RFC-003
    /// ("o orçamento de ciclos já resolve recursão infinita sozinho") é
    /// falsa se invocar for de graça: sem `USER_CALL_COST`, `f()` chamando
    /// `f()` nunca gastaria ciclo, nunca estouraria o orçamento, e
    /// recorreria até estourar a pilha do Rust — um abort de processo que
    /// `Result` não captura e que fecharia o jogo. Este teste prova que,
    /// com o custo por invocação, a recursão trunca como qualquer outro
    /// loop longo: `run_turn` retorna normalmente com `Signal::Truncated`,
    /// e o processo de teste não crasha.
    #[test]
    fn infinite_recursion_truncates_by_budget_instead_of_crashing() {
        let src = "func f():\n    f()\n\nf()\n";
        let r = run(src, 10, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert!(r.truncated);
        assert!(r.cycles_used <= 10);
        assert!(matches!(r.events.last(), Some(TurnEvent::CounterAttack { .. })));
    }

    #[test]
    fn function_executes_body_and_applies_real_effect() {
        let src = "func combo():\n    atacar(magia[\"fogo\"])\n\ncombo()\n";
        let r = run(src, 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn function_calling_another_function_works() {
        let src = "func ajuda():\n    defender(escudo[\"ouro\"])\n\nfunc combo():\n    ajuda()\n    atacar(espada[\"ferro\"])\n\ncombo()\n";
        let r = run(src, 20, Weakness::ExigeGuarda, Posture::Guarda);
        assert!(matches!(r.events[0], TurnEvent::Defended { .. }));
        match &r.events[1] {
            TurnEvent::Attacked { effective, .. } => assert!(*effective),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn function_can_be_called_before_its_definition_in_text() {
        // regra 4: a coleta de funcoes acontece antes de rodar o programa,
        // entao a ordem no texto nao importa.
        let src = "combo()\n\nfunc combo():\n    atacar(magia[\"fogo\"])\n";
        let r = run(src, 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, .. } => assert!(*effective),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn function_call_cost_is_user_call_cost_plus_body_cost() {
        // RFC-024: cycles_used tambem inclui o custo de tamanho agora --
        // FuncDef(1) + corpo (atacar+defender, 2) + chamada combo() (1) = 4
        // `Stmt` -> STMT_SIZE_COST*4 = 8, somado uma vez no inicio do turno.
        let src = "func combo():\n    atacar(espada[\"ferro\"])\n    defender(escudo[\"ouro\"])\n\ncombo()\n";
        let r = run(src, 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert_eq!(r.cycles_used, api::USER_CALL_COST + 2 + 1 + 4 * api::STMT_SIZE_COST);
    }

    #[test]
    fn duplicate_function_name_errors_on_redefinition_line() {
        let program = parse("func combo():\n    esperar()\n\nfunc combo():\n    atacar(espada[\"ferro\"])\n\ncombo()\n").unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap_err();
        assert_eq!(err.line, 4);
        assert!(err.message.contains("combo"));
    }

    #[test]
    fn function_colliding_with_native_name_errors() {
        let program = parse("func atacar():\n    esperar()\n").unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap_err();
        assert!(err.message.contains("atacar"));
    }

    #[test]
    fn calling_user_function_with_argument_errors() {
        let program = parse("func combo():\n    esperar()\n\ncombo(1)\n").unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap_err();
        assert_eq!(err.line, 4);
        assert!(err.message.contains("argumento"));
    }

    #[test]
    fn call_depth_limit_triggers_before_a_generous_budget_runs_out() {
        // corrente linear de chamadas (nao recursiva) mais profunda que
        // MAX_CALL_DEPTH, com orcamento grande o bastante para nao truncar
        // por ciclo antes disso - prova que a rede de seguranca de
        // profundidade e o mecanismo disparando, nao o orcamento.
        let mut src = String::new();
        for i in 0..api::MAX_CALL_DEPTH + 2 {
            src.push_str(&format!("func f{i}():\n    f{}()\n\n", i + 1));
        }
        src.push_str(&format!("func f{}():\n    esperar()\n\n", api::MAX_CALL_DEPTH + 2));
        src.push_str("f0()\n");

        let program = parse(&src).unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 1000, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap_err();
        assert!(err.message.contains("profundidade"));
    }

    #[test]
    fn user_function_wins_against_beetle_within_budget_and_costs_exactly_one_more_than_inline() {
        // Escaravelho: vida 110, orcamento 17 (RFC-021 recalibrou os dois --
        // ver comentario em data.rs::beetle), fraqueza ExigeGuarda. O combo
        // so ataca quando a postura permite; sem esse `if` o script nao
        // vence (harness.md: "um script sem if nao vence"). A versao com
        // func e a equivalente sem func tomam exatamente a mesma decisao e
        // acertam o mesmo golpe efetivo — mas a versao com func gasta 1
        // ciclo mais (USER_CALL_COST) e por isso sobra 1 ciclo menos pro
        // golpe bonus no fim do turno (vm.rs: `remaining` vira dano extra).
        // O resultado final da vida do inimigo difere em exatamente 1
        // ponto de vida: a abstracao nunca e mais barata que o inline, nem
        // por acidente via o golpe bonus. (O orcamento calibrado em si nao
        // muda o resultado desta comparacao -- so precisa caber as duas
        // versoes sem truncar, o que qualquer orcamento >= 5 ja garante.)
        //
        // RFC-024: `with_func` tambem escreve 2 `Stmt` a mais que
        // `without_func` (o `FuncDef` e a chamada `combo()`, que nao existem
        // na versao inline) -- o custo de tamanho extra (2*STMT_SIZE_COST)
        // se soma ao USER_CALL_COST de execucao. A abstracao continua nunca
        // mais barata que o inline; agora ela e mais cara em duas frentes
        // (execucao E tamanho), o que so reforca a regra 3 da RFC (reusar
        // uma func e mais barato que reescrever, mas uma unica chamada sem
        // reuso paga o preco de tela por ela).
        let with_func = "func combo():\n    if inimigo.postura == \"guarda\":\n        atacar(espada[\"ferro\"])\n    else:\n        esperar()\n\ncombo()\n";
        let without_func = "if inimigo.postura == \"guarda\":\n    atacar(espada[\"ferro\"])\nelse:\n    esperar()\n";

        let budget = 17;
        let with = run(with_func, budget, Weakness::ExigeGuarda, Posture::Guarda);
        let without = run(without_func, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!with.truncated);
        assert!(!without.truncated);
        let extra = api::USER_CALL_COST + 2 * api::STMT_SIZE_COST;
        assert_eq!(with.cycles_used, without.cycles_used + extra);
        assert_eq!(with.enemy_life, without.enemy_life + extra as i32);

        let effective_hit = |r: &TurnResult| match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => (*effective, *damage),
            other => panic!("evento inesperado: {other:?}"),
        };
        assert_eq!(effective_hit(&with), effective_hit(&without));
        assert_eq!(effective_hit(&with), (true, BASE_ATTACK_DAMAGE));
    }

    // RFC-008 — Guardiao das Duas Chaves (Weakness::DuploSelo)

    #[test]
    fn duplo_selo_deals_full_damage_only_with_guard_and_inspection_together() {
        let src = "inspecionar()\natacar(espada.Bronze)\n";
        let r = run(src, 22, Weakness::DuploSelo, Posture::Guarda);
        match &r.events[1] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn duplo_selo_missing_inspection_deals_reduced_damage() {
        // guarda certa, mas sem inspecionar -- so uma das duas condicoes
        let src = "atacar(espada.Bronze)\n";
        let r = run(src, 22, Weakness::DuploSelo, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn duplo_selo_missing_guard_deals_reduced_damage() {
        // inspecionou, mas postura aberta -- so a outra condicao isolada
        let src = "inspecionar()\natacar(espada.Bronze)\n";
        let r = run(src, 22, Weakness::DuploSelo, Posture::Aberta);
        match &r.events[1] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn duplo_selo_reference_script_wins_within_calibrated_budget() {
        // Script de referencia da RFC-008 (secao "Efeito no pseudo-codigo"):
        // inspeciona sempre e ataca so na guarda, espera na aberta. Custo
        // pior caso (postura guarda): inspecionar (3) + if (1, BRANCH_COST)
        // + atacar (2) = 6 ciclos; postura aberta: 3 + 1 + esperar (1) = 5.
        // Orcamento fixo deste teste (10, valor original da RFC-008): cabe
        // o pior caso com folga pequena (4 ciclos de bonus). RFC-022 depois
        // subiu o orcamento real do bestiario (data.rs::guardiao()) para 12
        // -- este teste mantem 10 fixo, decoupled do bestiario atual, so
        // pra provar que o script de referencia sempre vence dentro de um
        // orcamento apertado; o ritmo real do Aker calibrado esta em
        // `guardiao_rhythm_within_target_range`.
        //
        // RFC-024: 4 `Stmt` (inspecionar + if + atacar/esperar nos dois
        // ramos) -> custo de tamanho +8, cobrado uma vez por turno.
        // Orcamento sobe de 10 para 18 pra preservar a mesma folga de antes
        // desta RFC no pior caso (guarda: 3+1+2=6 de execucao + 8 de
        // tamanho = 14, folga de 4 ciclos, igual a antes).
        let src = "inspecionar()\nif inimigo.postura == \"guarda\":\n    atacar(espada.Bronze)\nelse:\n    esperar()\n";
        let budget = 18;
        let mut life = 150;
        let mut posture = Posture::Guarda;
        let mut turns = 0;
        while life > 0 && turns < 200 {
            let program = parse(src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 150, posture, Weakness::DuploSelo, 9, false).unwrap();
            assert!(!r.truncated, "script de referencia nao pode estourar o orcamento calibrado");
            life = r.enemy_life;
            posture = posture.toggled();
            turns += 1;
        }
        assert_eq!(life, 0, "script de referencia precisa vencer o Guardiao dentro do orcamento calibrado");
        assert!(turns > 1, "vitoria em 1 turno indicaria orcamento excessivo (sobra demais)");
    }

    #[test]
    fn duplo_selo_naive_spam_never_beats_composed_script_rfc_011() {
        // RFC-011: o QA achou, e o product-manager confirmou por calculo
        // independente, que a reducao /4 original permitia um antijogo real
        // -- um script que so ataca sem compor postura+inspecao vencia Aker
        // (Weakness::DuploSelo) em MENOS turnos que o script correto. Este
        // teste e a defesa permanente: qualquer recalibracao futura que
        // reabra essa lacuna quebra o CI.
        //
        // Estrategia ingenua: atacar() x5 por turno, sem inspecionar() nem
        // compor com `if`. Custo: 5 x 2 ciclos = 10 (exato, orcamento
        // calibrado do Guardiao). `enemy_inspected` comeca falso a cada
        // turno (Vm::new) e a estrategia nunca inspeciona -- logo a condicao
        // composta de DuploSelo nunca vale, e todo golpe usa a reducao:
        // 5 x (BASE_ATTACK_DAMAGE=12 / 8) = 5 x 1 = 5 dano/turno.
        // 150 vida / 5 dano/turno = 30 turnos.
        //
        // RFC-024: cada script agora tambem paga STMT_SIZE_COST por Stmt
        // escrito, uma vez por turno, antes da execucao. Somar esse custo de
        // tamanho ao orcamento de cada script (em vez de a um orcamento
        // compartilhado) preserva `remaining` -- e portanto o dano e a
        // contagem de turnos -- identicos a antes desta RFC:
        // `budget_novo = budget_antigo + count_stmts(script) * STMT_SIZE_COST`
        // faz `cycles_used_novo - budget_novo == cycles_used_antigo -
        // budget_antigo` para qualquer execucao do mesmo script. Ingenua: 5
        // `Stmt` -> +10; orcamento 10 -> 20.
        let naive_src = "atacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\n";
        let naive_budget = 20;
        let mut life = 150;
        let mut posture = Posture::Guarda;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), naive_budget, 100, 100, life, 150, posture, Weakness::DuploSelo, 9, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (5x atacar = 10 ciclos + 10 de tamanho) nao deveria estourar o orcamento de 20");
            life = r.enemy_life;
            posture = posture.toggled();
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Estrategia correta: mesmo script de referencia da RFC-008 --
        // inspeciona sempre, ataca so na guarda, espera na aberta. So a
        // reducao de dano do braco "condicoes nao compostas" mudou; o braco
        // de sucesso (BASE_ATTACK_DAMAGE cheio) e igual ao de antes, logo o
        // resultado continua ~15 turnos, nao mudou com a correcao do /8 nem
        // com o custo de tamanho (4 `Stmt` -> +8; orcamento 10 -> 18).
        let correct_src = "inspecionar()\nif inimigo.postura == \"guarda\":\n    atacar(espada.Bronze)\nelse:\n    esperar()\n";
        let correct_budget = 18;
        let mut life = 150;
        let mut posture = Posture::Guarda;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), correct_budget, 100, 100, life, 150, posture, Weakness::DuploSelo, 9, false).unwrap();
            assert!(!r.truncated, "script de referencia nao pode estourar o orcamento calibrado");
            life = r.enemy_life;
            posture = posture.toggled();
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script de referencia precisa vencer o Guardiao dentro do orcamento calibrado");

        // RFC-011: o script correto (compoe postura+inspecao) precisa vencer
        // em MENOS turnos que o spam ingenuo -- nunca igual, nunca mais.
        assert!(
            correct_turns < naive_turns,
            "antijogo: spam ingenuo ({naive_turns} turnos) deveria ser pior que o script correto ({correct_turns} turnos)"
        );
        // Confere os numeros recalculados na RFC-011: spam ~30 turnos (5
        // dano/turno com a reducao /8), script correto ~15 (inalterado).
        assert_eq!(naive_turns, 30, "spam ingenuo deveria levar ~30 turnos com a reducao /8");
        assert_eq!(correct_turns, 15, "script correto nao deveria ter mudado -- continua em ~15 turnos");
    }

    #[test]
    fn duplo_selo_label_covers_new_variant() {
        assert_eq!(Weakness::DuploSelo.label(), "EXIGE GUARDA E INSPECAO");
    }

    // --- RFC-010: estado persistente do jogador entre turnos ---

    #[test]
    fn player_var_survives_from_one_run_turn_call_to_the_next() {
        // Regra 1/3: `vars` e emprestado, nao recriado a cada `run_turn`.
        // Turno 1 escreve `x`; turno 2 (mesmo `vars`, mesma chamada de
        // `run_turn`, sem reescrever `x`) ainda le o valor gravado no
        // turno 1 -- e exatamente o criterio de aceite "grava no turno 1,
        // le no turno 3" da RFC, encurtado para 2 turnos.
        let write_src = "x = 42\nesperar()\n";
        let read_src = "if x == 42:\n    atacar(magia.Fogo)\nelse:\n    esperar()\n";
        let mut vars = HashMap::new();

        let program1 = parse(write_src).unwrap();
        let r1 = run_turn(&program1, &mut vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        assert!(!r1.truncated);
        assert_eq!(vars.get("x"), Some(&Value::Num(42.0)), "turno 1 devia deixar x=42 gravado no vars emprestado");

        let program2 = parse(read_src).unwrap();
        let r2 = run_turn(&program2, &mut vars, 20, 100, 100, r1.enemy_life, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        // se x nao tivesse sobrevivido, `x` seria variavel indefinida e o
        // script do turno 2 falharia com erro de runtime em vez de atacar.
        match &r2.events[0] {
            TurnEvent::Attacked { effective, .. } => assert!(*effective, "turno 2 devia ler x==42 e atacar de verdade"),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn new_duel_vars_start_empty_regardless_of_a_previous_map() {
        // Regra 3 (nao-objetivo 1): um `vars` novo (equivalente ao que
        // `DuelScene::new()` cria a cada duelo) nunca ve variavel de um
        // duelo anterior. Nao ha campo de reset explicito porque a posse
        // e nova a cada `DuelScene` -- este teste prova o efeito no nivel
        // da VM, que e onde a garantia realmente vive.
        let mut old_vars = HashMap::new();
        old_vars.insert("x".to_string(), Value::Num(42.0));
        drop(old_vars); // duelo anterior descartado, como ao sair de DuelScene

        let mut fresh_vars: HashMap<String, Value> = HashMap::new();
        let program = parse("if x == 42:\n    atacar(magia.Fogo)\nelse:\n    esperar()\n").unwrap();
        let err = run_turn(&program, &mut fresh_vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false);
        assert!(err.is_err(), "vars novo nao pode conhecer 'x' de um duelo anterior");
    }

    #[test]
    fn dry_run_probe_never_leaks_a_write_the_real_pass_never_executes() {
        // O teste mais importante desta RFC (regra 2). `atacar()` so
        // decrementa `enemy_life` fora do dry-run (vm.rs, ramo "atacar" em
        // `eval_native_call`, guardado por `!self.dry_run`). Com um
        // inimigo de vida 10 e ataque efetivo (12 de dano), a passada REAL
        // mata o inimigo -- `inimigo.vida` cai para 0 -- mas a passada de
        // VALIDACAO nao muda `enemy_life` (efeito colateral suprimido),
        // entao pra ela o inimigo "continua vivo" com vida 10.
        //
        // O `if inimigo.vida > 0:` depois do ataque so e verdadeiro na
        // passada de validacao. Sem a regra 2 (clone), essa escrita de
        // `x` feita SO no dry-run vazaria direto pro `vars` do jogador --
        // mesmo o ramo nunca tendo executado de verdade, porque na
        // passada real o inimigo ja estava morto quando o `if` roda.
        let src = "atacar(magia.Fogo)\nif inimigo.vida > 0:\n    x = 1\n";
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();

        let r = run_turn(&program, &mut vars, 20, 100, 100, 10, 10, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();

        assert_eq!(r.enemy_life, 0, "ataque efetivo de 12 contra vida 10 devia matar o inimigo na passada real");
        assert!(
            !vars.contains_key("x"),
            "escrita feita so na passada de validacao (dry-run, que nao viu o inimigo morrer) vazou pro vars real"
        );
    }

    #[test]
    fn dry_run_probe_write_never_leaks_when_turn_errors_out() {
        // Variante do mesmo risco (regra 2): quando a passada de validacao
        // encontra um erro de verdade, `run_turn` devolve `Err` e a
        // passada real NUNCA RODA (retorno antecipado, run_turn acima).
        // Sem clonar `vars` antes do dry-run, a escrita de `x = 1` (que
        // roda antes do erro) teria vazado pro `vars` do chamador mesmo
        // com o turno inteiro invalidado -- contradizendo o comentario
        // original do modulo: "[erro] devolve sem mexer no HP de
        // ninguem", que esta RFC estende a `vars`.
        let src = "x = 1\natacar(naoexiste)\n";
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();

        let err = run_turn(&program, &mut vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false);

        assert!(err.is_err());
        assert!(vars.is_empty(), "turno que erra nao pode deixar nenhuma escrita de variavel vazada no vars real");
    }

    // RFC-012 — Sentinela das Palavras Verdadeiras (Weakness::ExigeNomeacao)

    #[test]
    fn exige_nomeacao_deals_full_damage_when_attack_runs_inside_a_func() {
        let src = "func golpe():\n    atacar(espada.Bronze)\n\ngolpe()\n";
        let r = run(src, 16, Weakness::ExigeNomeacao, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn exige_nomeacao_deals_reduced_damage_when_attack_runs_at_top_level() {
        // mesmo item, mesmo golpe -- so que solto no corpo principal do
        // script (depth == 0), sem nenhuma func no caminho de execucao.
        let src = "atacar(espada.Bronze)\n";
        let r = run(src, 16, Weakness::ExigeNomeacao, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8); // RFC-021: /4 -> /8
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn exige_nomeacao_label_covers_new_variant() {
        assert_eq!(Weakness::ExigeNomeacao.label(), "SO RESPEITA GOLPE NOMEADO");
    }

    #[test]
    fn exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns() {
        // RFC-012 regra 2 / criterio de aceite: simula a estrategia
        // ingenua mais forte (atacar() repetido no nivel superior, sem
        // func) contra a estrategia correta (mesmo atacar(), mas de
        // dentro de uma func chamada repetidamente, pagando
        // USER_CALL_COST a cada invocacao) e prova que a correta vence em
        // MENOS turnos, com margem clara -- mesmo padrao de teste
        // permanente que a RFC-011 exigiu depois do fato para o Aker,
        // aqui escrito antes de fechar os numeros (Sentinela, data.rs):
        // vida 150, orcamento 16.
        //
        // Ingenua: atacar() cabe 8x em 16 ciclos (8x2=16, sem sobra).
        // depth==0 o turno inteiro -> cada golpe usa a reducao (RFC-021: /8):
        // 8 x (BASE_ATTACK_DAMAGE=12 / 8) = 8 x 1 = 8 dano/turno.
        //
        // RFC-024: mesma tecnica de `duplo_selo_naive_spam_never_beats_
        // composed_script_rfc_011` -- somar STMT_SIZE_COST*count_stmts ao
        // orcamento de CADA script preserva `remaining` (e portanto o dano e
        // a contagem de turnos) identicos a antes desta RFC. Ingenua: 8
        // `Stmt` -> +16; orcamento 16 -> 32.
        let naive_src = "atacar(espada.Bronze)\n".repeat(8);
        let naive_budget = 32;
        let mut life = 150;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), naive_budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeNomeacao, 8, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (8x atacar = 16 ciclos + 16 de tamanho) nao deveria estourar o orcamento de 32");
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: golpe() custa USER_CALL_COST(1) + atacar(2) = 3 ciclos
        // por invocacao; cabe 5x em 16 (5x3=15, sobra 1 -> golpe bonus de
        // fim de turno). depth>0 dentro do corpo de golpe() -> dano cheio:
        // 5 x 12 + 1 (bonus) = 61 dano/turno. Tamanho: FuncDef(1) + corpo(1)
        // + 5 chamadas = 7 `Stmt` -> +14; orcamento 16 -> 30.
        let correct_src = "func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(5);
        let correct_budget = 30;
        let mut life = 150;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), correct_budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeNomeacao, 8, false).unwrap();
            assert!(!r.truncated, "script com func nao pode estourar o orcamento calibrado de 30");
            life = r.enemy_life;
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script com func precisa vencer o Sentinela dentro do orcamento calibrado");

        assert!(
            correct_turns < naive_turns,
            "a estrategia com func ({correct_turns} turnos) precisa vencer em menos turnos que o spam ingenuo ({naive_turns} turnos)"
        );
        assert!(
            naive_turns >= correct_turns * 2,
            "margem fraca: ingenua {naive_turns} turnos vs correta {correct_turns} turnos -- nao pode ser um empate raso"
        );
    }

    // --- RFC-017: ExigeInvocacaoDupla (7o monstro, fecha o ciclo de invocar) --

    #[test]
    fn attack_with_two_invocations_this_turn_deals_full_damage() {
        // RFC-024: 5 `Stmt` (2 `invocar` de 2 cada + 1 `atacar`) -> +10 de
        // tamanho, somados aos 6 ciclos de execucao (2*INVOKE_COST=4 +
        // atacar=2) = 16; orcamento sobe de 12 para 22 pra caber com folga.
        let src = "invocar a:\n    esperar()\ninvocar b:\n    esperar()\natacar(espada.Bronze)\n";
        let r = run(src, 22, Weakness::ExigeInvocacaoDupla, Posture::Guarda);
        match r.events.iter().find(|e| matches!(e, TurnEvent::Attacked { .. })) {
            Some(TurnEvent::Attacked { effective, damage, .. }) => {
                assert!(*effective, "com 2 invocacoes no turno o ataque precisa ser efetivo");
                assert_eq!(*damage, BASE_ATTACK_DAMAGE);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn attack_with_zero_or_one_invocation_this_turn_deals_reduced_damage() {
        let sem_invocar = run("atacar(espada.Bronze)\n", 12, Weakness::ExigeInvocacaoDupla, Posture::Guarda);
        match &sem_invocar.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8); // RFC-021: /4 -> /8
            }
            other => panic!("evento inesperado: {other:?}"),
        }

        let uma_invocacao = run(
            "invocar a:\n    esperar()\natacar(espada.Bronze)\n",
            12,
            Weakness::ExigeInvocacaoDupla,
            Posture::Guarda,
        );
        match uma_invocacao.events.iter().find(|e| matches!(e, TurnEvent::Attacked { .. })) {
            Some(TurnEvent::Attacked { effective, damage, .. }) => {
                assert!(!*effective, "1 invocacao nao basta -- precisa das 2");
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8); // RFC-021: /4 -> /8
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn exige_invocacao_dupla_beats_naive_spam_in_fewer_turns() {
        // RFC-017 regra 1 / criterio de aceite: compara "ataca direto, sem
        // invocar" contra "invoca 2 vezes, depois ataca" e prova que a
        // segunda vence em MENOS turnos, com margem clara -- mesma
        // disciplina de teste de ordenacao obrigatoria desde a RFC-011/012,
        // escrita antes de fechar os numeros (Necroguardiao, data.rs): vida
        // 150, orcamento 12.
        //
        // Ingenua: atacar() cabe 6x em 12 ciclos (6x2=12, sem sobra).
        // invocations_this_turn == 0 o turno inteiro -> reducao (RFC-021: /8):
        // 6 x (BASE_ATTACK_DAMAGE=12 / 8) = 6 x 1 = 6 dano/turno.
        //
        // RFC-024: mesma tecnica de orcamentos separados (naive/correct) que
        // as demais bases de ordenacao -- soma STMT_SIZE_COST*count_stmts ao
        // orcamento de cada script, preservando `remaining` (e o resultado)
        // identico a antes desta RFC. Ingenua: 6 `Stmt` -> +12; 12 -> 24.
        let naive_src = "atacar(espada.Bronze)\n".repeat(6);
        let naive_budget = 24;
        let mut life = 150;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), naive_budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeInvocacaoDupla, 9, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (6x atacar = 12 ciclos + 12 de tamanho) nao deveria estourar o orcamento de 24");
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: 2x invocar (2*INVOKE_COST=4 ciclos no orcamento
        // principal) + atacar() cabe 4x nos 8 ciclos restantes (4x2=8, sem
        // sobra) = 12 ciclos, exatamente o orcamento. invocations_this_turn
        // == 2 antes do primeiro atacar() -> dano cheio:
        // 4 x BASE_ATTACK_DAMAGE(12) = 48 dano/turno. Tamanho: 2 `invocar`
        // (2 `Stmt` cada) + 4 `atacar()` = 8 `Stmt` -> +16; 12 -> 28.
        let correct_src = "invocar a:\n    esperar()\ninvocar b:\n    esperar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(4);
        let correct_budget = 28;
        let mut life = 150;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), correct_budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeInvocacaoDupla, 9, false).unwrap();
            assert!(!r.truncated, "2 invocacoes + 4 atacar() (12 ciclos + 16 de tamanho) nao pode estourar o orcamento calibrado de 28");
            life = r.enemy_life;
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script com 2 invocacoes precisa vencer o Necroguardiao dentro do orcamento calibrado");

        assert!(
            correct_turns < naive_turns,
            "a estrategia com 2 invocacoes ({correct_turns} turnos) precisa vencer em menos turnos que o spam ingenuo ({naive_turns} turnos)"
        );
        assert!(
            naive_turns >= correct_turns * 2,
            "margem fraca: ingenua {naive_turns} turnos vs correta {correct_turns} turnos -- nao pode ser um empate raso"
        );
    }

    // RFC-002 — inventário real: um item equipado muda o dano de verdade.

    fn run_with_loadout(src: &str, budget: u32, weakness: Weakness, posture: Posture, loadout: Option<&Loadout>) -> TurnResult {
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn_with_loadout(&program, &mut vars, budget, 100, 100, 100, 100, posture, weakness, 10, false, loadout).unwrap()
    }

    fn loadout_with_sword(name: &str, bonus_damage: i32) -> Loadout {
        Loadout {
            arma: Some(crate::inventory::Item { id: "teste".into(), kind: ItemKind::Espada, name: name.into(), bonus_damage }),
            magia: None,
            escudo: None,
            pocao: None,
        }
    }

    #[test]
    fn equipped_item_bonus_damage_changes_final_damage_dealt() {
        // mesmo script, mesmo monstro: só o bônus do item equipado muda.
        let src = "atacar(espada.Ferro)\n";
        let low = loadout_with_sword("ferro", 3);
        let high = loadout_with_sword("ferro", 9);

        // budget == custo exato de atacar() (2 ciclos): sem ciclo sobrando,
        // sem golpe bonus de fim de turno a misturar no dano medido.
        let r_low = run_with_loadout(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(&low));
        let r_high = run_with_loadout(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(&high));

        // Weakness::ExigeGuarda em Posture::Guarda -> dano cheio
        // (BASE_ATTACK_DAMAGE=12) + bônus do item equipado.
        assert_eq!(100 - r_low.enemy_life, 12 + 3);
        assert_eq!(100 - r_high.enemy_life, 12 + 9);
        assert!(r_high.enemy_life < r_low.enemy_life, "item com bonus_damage maior precisa causar mais dano final");
    }

    #[test]
    fn equipped_item_bonus_is_case_insensitive_on_name_match() {
        let src = "atacar(espada.Ferro)\n";
        let loadout = loadout_with_sword("FERRO", 5);
        let r = run_with_loadout(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(&loadout));
        assert_eq!(100 - r.enemy_life, 12 + 5, "nome do item equipado deve casar case-insensitive com o nome citado no script");
    }

    #[test]
    fn no_loadout_behaves_identically_to_pre_rfc_002() {
        // sem inventário nenhum, run_turn_with_loadout(None) precisa
        // devolver exatamente o que run_turn (assinatura antiga) devolve.
        let program = parse("atacar(magia.Fogo)\n").unwrap();
        let mut vars_a = HashMap::new();
        let mut vars_b = HashMap::new();
        let via_old = run_turn(&program, &mut vars_a, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        let via_new =
            run_turn_with_loadout(&program, &mut vars_b, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, None)
                .unwrap();
        assert_eq!(via_old, via_new);
    }

    #[test]
    fn equipped_item_in_wrong_slot_or_wrong_name_grants_no_bonus() {
        let src = "atacar(espada.Ferro)\n";
        // item equipado no slot certo, mas nome diferente do citado no script.
        let wrong_name = loadout_with_sword("bronze", 20);
        let r = run_with_loadout(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(&wrong_name));
        assert_eq!(100 - r.enemy_life, 12, "nome diferente do equipado nao deve conceder bonus (item ausente/errado nunca e erro, so bonus zero)");
    }

    // RFC-003 §1 — itens por classe: bônus aditivo de `PlayerClass`.

    fn run_with_class(src: &str, budget: u32, weakness: Weakness, posture: Posture, player_class: Option<PlayerClass>) -> TurnResult {
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn_with_loadout_and_class(&program, &mut vars, budget, 100, 100, 100, 100, posture, weakness, 10, false, None, player_class).unwrap()
    }

    #[test]
    fn guerreiro_attacking_with_espada_deals_more_damage_than_without_class() {
        // budget == custo exato de atacar() (2 ciclos): sem ciclo sobrando,
        // sem golpe bonus de fim de turno a misturar no dano medido.
        let src = "atacar(espada.Ferro)\n";
        let sem_classe = run_with_class(src, 4, Weakness::ExigeGuarda, Posture::Guarda, None);
        let guerreiro = run_with_class(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(PlayerClass::Guerreiro));

        assert_eq!(100 - sem_classe.enemy_life, 12, "sem classe, dano deve ser o dano base de atacar() em Weakness::ExigeGuarda/Posture::Guarda");
        assert_eq!(100 - guerreiro.enemy_life, 12 + api::CLASS_BONUS_DAMAGE, "Guerreiro atacando com espada deve receber CLASS_BONUS_DAMAGE");
        assert!(guerreiro.enemy_life < sem_classe.enemy_life, "Guerreiro com espada precisa causar mais dano final que sem classe escolhida");
    }

    #[test]
    fn guerreiro_attacking_with_magia_gets_no_class_bonus() {
        // afinidade do Guerreiro e Espada, nao Magia -- atacar com magia
        // nao deve conceder CLASS_BONUS_DAMAGE mesmo com classe escolhida.
        let src = "atacar(magia.Fogo)\n";
        let sem_classe = run_with_class(src, 2, Weakness::Elemento(Element::Fogo), Posture::Guarda, None);
        let guerreiro = run_with_class(src, 2, Weakness::Elemento(Element::Fogo), Posture::Guarda, Some(PlayerClass::Guerreiro));
        assert_eq!(
            sem_classe.enemy_life, guerreiro.enemy_life,
            "afinidade nao bate (Guerreiro afina com Espada, nao Magia) -- dano deve ser identico ao caso sem classe"
        );
    }

    #[test]
    fn no_player_class_behaves_identically_to_pre_rfc_003() {
        // sem classe escolhida, run_turn_with_loadout_and_class(None) precisa
        // devolver exatamente o que run_turn_with_loadout (assinatura
        // anterior a esta RFC) devolve.
        let program = parse("atacar(magia.Fogo)\n").unwrap();
        let mut vars_a = HashMap::new();
        let mut vars_b = HashMap::new();
        let via_old =
            run_turn_with_loadout(&program, &mut vars_a, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, None)
                .unwrap();
        let via_new = run_turn_with_loadout_and_class(
            &program,
            &mut vars_b,
            20,
            100,
            100,
            100,
            100,
            Posture::Guarda,
            Weakness::Elemento(Element::Fogo),
            10,
            false,
            None,
            None,
        )
        .unwrap();
        assert_eq!(via_old, via_new);
    }

    #[test]
    fn mago_attacking_with_magia_deals_more_damage_than_without_class() {
        let src = "atacar(magia.Fogo)\n";
        let sem_classe = run_with_class(src, 4, Weakness::Elemento(Element::Fogo), Posture::Guarda, None);
        let mago = run_with_class(src, 4, Weakness::Elemento(Element::Fogo), Posture::Guarda, Some(PlayerClass::Mago));
        assert_eq!(100 - mago.enemy_life, (100 - sem_classe.enemy_life) + api::CLASS_BONUS_DAMAGE);
    }

    #[test]
    fn ladrao_attacking_with_pocao_deals_more_damage_than_without_class() {
        // ladrao ataca com pocao (RFC-003: afinidade tematica, nao ha
        // restricao de atacar com pocao na linguagem -- e so um ItemKind).
        let src = "atacar(pocao.Vida)\n";
        let sem_classe = run_with_class(src, 4, Weakness::ExigeGuarda, Posture::Guarda, None);
        let ladrao = run_with_class(src, 4, Weakness::ExigeGuarda, Posture::Guarda, Some(PlayerClass::Ladrao));
        assert_eq!(100 - ladrao.enemy_life, (100 - sem_classe.enemy_life) + api::CLASS_BONUS_DAMAGE);
    }

    // Critério de aceite da RFC-003: os 4 testes de ordenação entre
    // estratégias (RFC-008/011/012) continuam passando mesmo com uma
    // classe hipotética escolhida no cenário -- prova que o bônus aditivo
    // nao muda a relação de turnos entre estratégias. Reexecuta o mesmo
    // cenário de `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns`
    // com `Some(PlayerClass::Guerreiro)` (afinidade Espada, presente nos
    // dois scripts) via `run_turn_with_loadout_and_class`, sem alterar
    // nenhuma assertion do teste original.
    #[test]
    fn exige_nomeacao_named_func_beats_naive_spam_with_class_bonus_still_holds() {
        // RFC-024: mesmos orcamentos separados (naive/correct) que
        // `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns` usa --
        // ver o comentario la para a derivacao (16 -> 32 ingenua, 16 -> 30
        // correta).
        let naive_src = "atacar(espada.Bronze)\n".repeat(8);
        let naive_budget = 32;
        let mut life = 150;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let mut vars = HashMap::new();
            let r = run_turn_with_loadout_and_class(
                &program,
                &mut vars,
                naive_budget,
                100,
                100,
                life,
                150,
                Posture::Guarda,
                Weakness::ExigeNomeacao,
                8,
                false,
                None,
                Some(PlayerClass::Guerreiro),
            )
            .unwrap();
            assert!(!r.truncated);
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0);

        let correct_src = "func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(5);
        let correct_budget = 30;
        let mut life = 150;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let mut vars = HashMap::new();
            let r = run_turn_with_loadout_and_class(
                &program,
                &mut vars,
                correct_budget,
                100,
                100,
                life,
                150,
                Posture::Guarda,
                Weakness::ExigeNomeacao,
                8,
                false,
                None,
                Some(PlayerClass::Guerreiro),
            )
            .unwrap();
            assert!(!r.truncated);
            life = r.enemy_life;
            correct_turns += 1;
        }
        assert_eq!(life, 0);

        assert!(correct_turns < naive_turns, "bonus de classe aditivo nao pode reabrir o antijogo de spam ingenuo travado pela RFC-012");
    }

    // RFC-014 — bonus de item/classe tambem em curar(): reaproveita
    // item_bonus (equipped_bonus + class_bonus) ja usado por resolve_attack.

    fn loadout_with_potion(name: &str, bonus_damage: i32) -> Loadout {
        Loadout {
            arma: None,
            magia: None,
            escudo: None,
            pocao: Some(crate::inventory::Item { id: "teste".into(), kind: ItemKind::Pocao, name: name.into(), bonus_damage }),
        }
    }

    // player_life = 50, player_max_life = 100: bem longe do teto, sem
    // risco de o `.min()` mascarar diferenca de bonus entre os casos.
    // budget == custo exato de curar() (4 ciclos): sem golpe bonus de fim
    // de turno a misturar na cura medida.
    fn run_curar(budget: u32, loadout: Option<&Loadout>, player_class: Option<PlayerClass>) -> TurnResult {
        let program = parse("curar(pocao.Vida)\n").unwrap();
        let mut vars = HashMap::new();
        run_turn_with_loadout_and_class(
            &program,
            &mut vars,
            budget,
            50,
            100,
            100,
            100,
            Posture::Guarda,
            Weakness::ExigeGuarda,
            10,
            false,
            loadout,
            player_class,
        )
        .unwrap()
    }

    #[test]
    fn equipped_potion_bonus_damage_heals_more() {
        let sem_pocao = run_curar(6, None, None);
        let com_pocao = run_curar(6, Some(&loadout_with_potion("vida", 6)), None);

        // RFC-025 regra 1: `run_curar` nao chama defender(), entao o dano
        // do turno (enemy_base_damage=10, sem carga cheia) sempre desconta
        // por cima da cura -- subtrai 10 do que seria so `HEAL_AMOUNT`.
        assert_eq!(sem_pocao.player_life, 50 + HEAL_AMOUNT - 10, "sem pocao equipada, cura deve ser exatamente HEAL_AMOUNT menos o dano do turno");
        assert_eq!(com_pocao.player_life, 50 + HEAL_AMOUNT + 6 - 10, "pocao equipada com bonus_damage deve curar mais");
        assert!(com_pocao.player_life > sem_pocao.player_life, "pocao com bonus_damage maior precisa curar mais que sem pocao equipada");
    }

    #[test]
    fn ladrao_using_curar_gets_class_bonus_guerreiro_does_not() {
        let sem_classe = run_curar(6, None, None);
        let ladrao = run_curar(6, None, Some(PlayerClass::Ladrao));
        let guerreiro = run_curar(6, None, Some(PlayerClass::Guerreiro));

        assert_eq!(ladrao.player_life, sem_classe.player_life + api::CLASS_BONUS_DAMAGE, "Ladrao usando curar() com pocao deve receber CLASS_BONUS_DAMAGE");
        assert_eq!(guerreiro.player_life, sem_classe.player_life, "afinidade do Guerreiro e Espada, nao Pocao -- curar() nao deve conceder bonus");
    }

    #[test]
    fn curar_without_item_or_class_heals_exactly_heal_amount() {
        // sem loadout e sem classe: mesmo comportamento de antes da RFC-014,
        // menos o dano do turno que a RFC-025 passou a aplicar sempre (10,
        // ver `run_curar`).
        let r = run_curar(6, None, None);
        assert_eq!(r.player_life, 50 + HEAL_AMOUNT - 10, "sem item/classe, curar() deve curar exatamente HEAL_AMOUNT, igual ao comportamento pre-RFC-014, menos o dano do turno da RFC-025");
    }

    // RFC-016 — bonus de item/classe tambem em defender(): o item usado no
    // ultimo defender() do turno reduz o contra-ataque bloqueado, alem do
    // corte de 50% ja existente. Mesma fonte (`item_bonus`) que
    // `resolve_attack` e `curar()` ja usam.

    fn loadout_with_shield(name: &str, bonus_damage: i32) -> Loadout {
        Loadout {
            arma: None,
            magia: None,
            escudo: Some(crate::inventory::Item { id: "teste".into(), kind: ItemKind::Escudo, name: name.into(), bonus_damage }),
            pocao: None,
        }
    }

    // enemy_base_damage = 10 (mesmo valor dos outros helpers de teste):
    // bloqueado sem bonus reduz para 10/2 = 5. Budget curto o bastante pra
    // truncar via `while` depois do(s) `defender()`, sem golpe bonus de
    // fim de turno a misturar no dano medido.
    fn run_defender(budget: u32, src: &str, loadout: Option<&Loadout>) -> TurnResult {
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn_with_loadout(&program, &mut vars, budget, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, loadout).unwrap()
    }

    fn counter_damage(r: &TurnResult) -> i32 {
        match r.events.last() {
            Some(TurnEvent::CounterAttack { damage, blocked, .. }) => {
                assert!(*blocked, "teste espera contra-ataque bloqueado");
                *damage
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn shield_with_bigger_bonus_reduces_blocked_counterattack_more() {
        let src = "defender(escudo.Ouro)\nwhile inimigo.vida > 0:\n    atacar(espada[\"ferro\"])\n";
        let low = loadout_with_shield("ouro", 1);
        let high = loadout_with_shield("ouro", 3);

        let r_low = run_defender(7, src, Some(&low));
        let r_high = run_defender(7, src, Some(&high));

        // RFC-025 regra 3: turno truncado dobra o dano antes de aplicar
        // defender() -- base bloqueada = (10*TRUNCATE_DAMAGE_MULTIPLIER)/2
        // = 10; bonus subtrai por cima, piso em 0.
        assert_eq!(counter_damage(&r_low), 10 - 1);
        assert_eq!(counter_damage(&r_high), 10 - 3);
        assert!(counter_damage(&r_high) < counter_damage(&r_low), "escudo com bonus_damage maior precisa reduzir mais o contra-ataque bloqueado");
    }

    #[test]
    fn defender_without_item_bonus_reduces_exactly_fifty_percent() {
        // sem loadout equipado: comportamento identico ao pre-RFC-016.
        let src = "defender(escudo.Ouro)\nwhile inimigo.vida > 0:\n    atacar(espada[\"ferro\"])\n";
        let r = run_defender(7, src, None);
        // RFC-025: dano pre-defesa dobra por causa do truncamento (10*2=20);
        // defender() sem bonus continua reduzindo exatamente 50% disso (10).
        assert_eq!(counter_damage(&r), 10, "sem item equipado, defender() deve reduzir exatamente 50% do dano (ja dobrado pelo truncamento)");
    }

    #[test]
    fn second_defender_call_in_same_turn_overwrites_the_first_for_bonus_purposes() {
        // escudo equipado eh "ouro" (bonus 6): se o primeiro defender()
        // (com "ouro") fosse o que contasse, o bonus se aplicaria e o dano
        // cairia a 0. A regra e' que só o ULTIMO conta -- e o ultimo usa
        // "prata", que nao bate com o equipado, bonus zero, dano = 5.
        // RFC-024: 4 `Stmt` (2 defender + 1 while + 1 atacar no corpo) -> +8
        // de tamanho, somado ao orcamento pra preservar a mesma dinamica de
        // truncamento de antes desta RFC (8 -> 16).
        let src = "defender(escudo.Ouro)\ndefender(escudo.Prata)\nwhile inimigo.vida > 0:\n    atacar(espada[\"ferro\"])\n";
        let loadout = loadout_with_shield("ouro", 6);
        let r = run_defender(16, src, Some(&loadout));
        assert_eq!(counter_damage(&r), 10, "so o ultimo defender() do turno deve contar para o bonus (base 10 = 20 dobrado pelo truncamento, /2 pelo bloqueio)");
    }

    // --- RFC-004: invocar (threads de invocacao do necromante) --------

    #[test]
    fn invoke_single_attack_deals_real_damage() {
        // RFC-024: 2 `Stmt` (invocar + atacar no corpo) -> +4 de tamanho,
        // somados ao INVOKE_COST(2) de execucao no orcamento principal;
        // orcamento sobe de 4 para 8.
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\n";
        let r = run(src, 8, Weakness::ExigeGuarda, Posture::Guarda);
        assert!(r.enemy_life < 100, "atacar() dentro de invocar precisa causar dano real ao inimigo");
        assert!(matches!(r.events[0], TurnEvent::Attacked { .. }));
    }

    /// O teste mais importante desta RFC (ver Nota de investigacao): prova
    /// que execucao sequencial com troca de contador de ciclos, sem
    /// suspensao/reescrita de `exec_block`/`exec_stmt`, basta para "duas
    /// threads atacando no mesmo turno, dano somado" -- a decisao de
    /// arquitetura da RFC-004 funciona sem precisar de mudanca estrutural
    /// maior na VM.
    #[test]
    fn two_invocations_in_same_turn_sum_damage_to_enemy() {
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\ninvocar mago_morto:\n    atacar(magia.Fogo)\n";
        // orcamento principal == exatamente 2*INVOKE_COST + custo de
        // tamanho: paga as duas invocacoes e nada mais, sem sobra pra golpe
        // bonus interferir na medicao do dano. RFC-024: 4 `Stmt` (2
        // `invocar` de 2 cada) -> +8 de tamanho.
        let budget = 2 * api::INVOKE_COST + 4 * api::STMT_SIZE_COST;
        let r = run(src, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!r.truncated, "duas invocacoes dentro do orcamento principal nao podem truncar o turno");
        // RFC-025 regra 1: o monstro ataca todo turno agora, truncado ou
        // nao -- o CounterAttack existe sempre, so `truncated` no evento
        // muda (aqui, false: o dano nao foi dobrado pela punicao de
        // truncamento).
        assert!(
            r.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { truncated: false, .. })),
            "sem truncamento do turno principal o ataque do monstro ainda acontece, so nao dobrado: {:?}",
            r.events
        );
        // ExigeGuarda com postura em Guarda: os dois ataques sao efetivos,
        // dano cheio cada -- a soma prova que o dano das duas invocacoes
        // realmente se acumula no mesmo `enemy_life`.
        assert_eq!(r.enemy_life, 100 - 2 * BASE_ATTACK_DAMAGE);
        let attacks = r.events.iter().filter(|e| matches!(e, TurnEvent::Attacked { .. })).count();
        assert_eq!(attacks, 2, "as duas invocacoes precisam ter executado seu atacar(): {:?}", r.events);
    }

    #[test]
    fn invoke_budget_overflow_does_not_truncate_turn_or_counterattack() {
        // corpo com 3 atacar() (6 ciclos) estoura INVOKE_BUDGET (4) na
        // terceira chamada -- a invocacao trunca internamente, mas o
        // script principal continua depois do `invocar` e chega ao
        // `esperar()` final.
        let src = "invocar zumbi:\n    atacar(espada.Ferro)\n    atacar(espada.Ferro)\n    atacar(espada.Ferro)\nesperar()\n";
        // RFC-024: 5 `Stmt` (invocar(1) + 3 atacar no corpo + esperar) -> +10
        // de tamanho, somado ao que ja cobria so invocar + esperar().
        let budget = api::INVOKE_COST + 1 + 5 * api::STMT_SIZE_COST;
        let r = run(src, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!r.truncated, "estouro de orcamento dentro de invocar nao pode truncar o turno principal");
        assert!(
            !r.events.iter().any(|e| matches!(e, TurnEvent::Truncated { .. })),
            "truncamento de invocacao nao pode gerar TurnEvent::Truncated do turno: {:?}",
            r.events
        );
        // RFC-025 regra 1: o CounterAttack do turno acontece de qualquer
        // jeito (o monstro ataca todo turno) -- o que o truncamento
        // *interno* da invocacao nao pode fazer e dobrar esse dano
        // (`truncated: false` no evento, nao ausencia dele).
        assert!(
            r.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { truncated: false, .. })),
            "truncamento de invocacao nao pode disparar o contra-ataque *dobrado* do turno: {:?}",
            r.events
        );
        assert!(
            r.events.iter().any(|e| matches!(e, TurnEvent::Waited { .. })),
            "o script principal precisa continuar depois do invocar truncado e chegar no esperar(): {:?}",
            r.events
        );
        // so 2 dos 3 atacar() do corpo couberam no INVOKE_BUDGET antes de
        // truncar -- a terceira chamada nunca chega a acontecer.
        let attacks = r.events.iter().filter(|e| matches!(e, TurnEvent::Attacked { .. })).count();
        assert_eq!(attacks, 2, "so 2 atacar() cabem em INVOKE_BUDGET antes do estouro interno: {:?}", r.events);
    }

    #[test]
    fn third_invocation_in_same_turn_is_a_clear_error_on_the_right_line() {
        let src = "invocar a:\n    esperar()\ninvocar b:\n    esperar()\ninvocar c:\n    esperar()\n";
        let program = parse(src).unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 100, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false)
            .unwrap_err();
        assert_eq!(err.line, 5, "a terceira invocacao (linha do 'invocar c:') e a que excede o limite");
        assert!(err.message.contains("invoca"), "mensagem de erro precisa falar de invocacao: {}", err.message);
    }

    #[test]
    fn real_error_inside_invoke_propagates_and_invalidates_turn() {
        // funcao desconhecida dentro do corpo de invocar e um erro real
        // (Signal::Error), nao um truncamento de orcamento -- precisa
        // propagar e invalidar o turno, igual a um erro em qualquer outro
        // lugar do script (`runtime_error_does_not_consume_turn`).
        let program = parse("invocar esqueleto:\n    fantasma()\n").unwrap();
        let err = run_turn(&program, &mut HashMap::new(), 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false);
        assert!(err.is_err(), "erro real dentro de invocar precisa invalidar o turno");
    }

    #[test]
    fn func_inside_invoke_is_rejected_same_as_other_blocks() {
        // regra 7: `invocar` reaproveita `block()`, que ja incrementa
        // `block_depth` -- `func` dentro de `invocar` cai na mesma checagem
        // que ja rejeita `func` dentro de if/while/for, de graca.
        let src = "invocar esqueleto:\n    func f():\n        esperar()\n    f()\n";
        let err = parse(src).unwrap_err();
        assert!(err.message.contains("func"), "mensagem precisa apontar 'func' como o problema: {}", err.message);
    }

    #[test]
    fn attack_inside_invoke_without_inner_func_against_apagado_deals_reduced_damage() {
        // regra 8: dentro de um `invocar` sem `func` interno, `self.depth`
        // continua 0 -- `eval_user_call` nunca roda para uma invocacao,
        // so para chamada de funcao do jogador. Contra ExigeNomeacao
        // (Apagado) isso significa dano reduzido, mesma consequencia de
        // chamar atacar() direto no nivel superior sem nomear uma func.
        // RFC-024: mesmo orcamento de `invoke_single_attack_deals_real_
        // damage` (2 `Stmt` -> +4 de tamanho, 4 -> 8).
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\n";
        let r = run(src, 8, Weakness::ExigeNomeacao, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective, "atacar() dentro de invocar sem func interno nao pode ser efetivo contra Apagado");
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 8); // RFC-021: /4 -> /8
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn reference_invoke_script_fits_every_current_monster_budget() {
        // Jogabilidade (criterio de aceite): as duas invocacoes do exemplo
        // da RFC, combinadas com um script principal razoavel, nao podem
        // estourar o orcamento principal de nenhum monstro do bestiario
        // atual. RFC-021 recalibrou o orcamento do Zumbi (8 -> 16, ver
        // `data.rs::zombie` e a bateria de testes de ordenacao logo abaixo)
        // para que `Weakness::Eficiencia` seja punivel de verdade. RFC-022
        // depois recalibrou os 7 pelo ritmo de combate -- o menor orcamento
        // do bestiario passa a ser o da Mumia (ver `data.rs::mummy`), nao
        // mais o do Aker.
        //
        // RFC-024: 5 `Stmt` (2 `invocar` de 2 cada + 1 `atacar`) -> +10 de
        // tamanho, somados aos 6 ciclos de execucao (2*INVOKE_COST + 1
        // atacar) = 16 ciclos no orcamento principal. `data.rs::mummy`
        // fechou seu `cycle_budget` em 16 (em vez do minimo exato de 12)
        // justamente pra este criterio continuar valendo -- ver o
        // comentario la.
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\ninvocar mago_morto:\n    atacar(magia.Fogo)\natacar(espada.Ferro)\n";
        let program = parse(src).unwrap();
        let budget = data::mummy().cycle_budget;
        let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        assert!(!r.truncated, "script de referencia com duas invocacoes nao pode estourar nem o menor orcamento do bestiario (Mumia, {budget})");
    }

    // --- RFC-015: selecionar() sobre a mochila ------------------------

    /// Passo 2 da RFC-015, isolado do resto: `Value::Item` como base de
    /// `eval_field` expõe `.nome`/`.tipo`/`.bonus` mesmo para um item
    /// construído pela sintaxe normal (nao vindo de `selecionar()`) —
    /// `.bonus` e 0 porque a regra 4 so preenche o valor real quando o
    /// item vem da mochila.
    #[test]
    fn item_field_access_exposes_nome_tipo_bonus() {
        let src = "x = espada.Ferro\nnome = x.nome\ntipo = x.tipo\nbonus = x.bonus\nesperar()\n";
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn(&program, &mut vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        assert_eq!(vars.get("nome"), Some(&Value::Str("ferro".to_string())));
        assert_eq!(vars.get("tipo"), Some(&Value::Str("espada".to_string())));
        assert_eq!(vars.get("bonus"), Some(&Value::Num(0.0)));
    }

    /// Monta uma `Bag` de teste a partir de `(kind, nome, bonus_damage)` —
    /// `id` reaproveita o nome (não importa pra `selecionar`, só identidade
    /// de save) e quantidade fixa em 1 (também irrelevante: regra 7 varre
    /// entradas, não unidades).
    fn bag_of(entries: Vec<(ItemKind, &str, i32)>) -> Bag {
        Bag(entries.into_iter().map(|(kind, name, bonus_damage)| (InvItem { id: name.to_string(), kind, name: name.to_string(), bonus_damage }, 1)).collect())
    }

    fn run_with_bag(src: &str, budget: u32, weakness: Weakness, posture: Posture, bag: &Bag) -> TurnResult {
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn_with_bag(&program, &mut vars, budget, 100, 100, 100, 100, posture, weakness, 10, false, None, None, Some(bag)).unwrap()
    }

    fn selected_event(r: &TurnResult) -> (usize, bool) {
        match r.events.iter().find_map(|e| match e {
            TurnEvent::Selected { examined, found, .. } => Some((*examined, *found)),
            _ => None,
        }) {
            Some(v) => v,
            None => panic!("nenhum TurnEvent::Selected encontrado: {:?}", r.events),
        }
    }

    /// Critério de aceite central da RFC-015: reordenar as cláusulas de
    /// `onde:` muda o custo real em ciclos, com o mesmo item encontrado no
    /// fim. Uma composição só de comparações simples (`item.tipo == ...`)
    /// não basta para provar isso: nenhuma delas custa ciclo pra avaliar
    /// (só instruções custam ciclo, `script/api.rs`), e o "and" é
    /// comutativo no valor booleano resultante — reordenar duas
    /// comparações puras nunca muda qual item é o primeiro a bater os dois
    /// filtros ao mesmo tempo, então o número de itens examinados pelo
    /// `selecionar` externo é idêntico nas duas ordens (ver nota de
    /// investigação: crítica de especificação sobre este ponto). A cláusula
    /// "cara" de verdade precisa custar ciclo pra avaliar — aqui ela é um
    /// `selecionar()` aninhado (varre a mochila de novo, cobrando
    /// `SELECT_SCAN_COST` por item que examina), o análogo direto de uma
    /// sub-consulta cara num `WHERE` de banco real. Com o filtro barato
    /// (`item.tipo == "pocao"`, comparação pura) primeiro, o curto-circuito
    /// de `and` (já existente em `eval_binary`) pula o `selecionar()`
    /// aninhado para os 3 itens que já falham no filtro barato; com o caro
    /// primeiro, o aninhado roda incondicionalmente pros 4 itens.
    #[test]
    fn onde_barato_antes_do_caro_custa_menos_ciclos_que_a_ordem_invertida_mesmo_resultado() {
        let bag = bag_of(vec![
            (ItemKind::Escudo, "a", 0),
            (ItemKind::Escudo, "b", 0),
            (ItemKind::Escudo, "c", 0),
            (ItemKind::Pocao, "d", 9),
        ]);

        // `selecionar(...)` sozinho numa posição booleana já converte via
        // `as_bool` (Item -> true, Nil -> false) — não precisa de um
        // literal `nil` pra comparar (a linguagem não tem um).
        let barato_primeiro = "item = selecionar(mochila, onde: item.tipo == \"pocao\" and selecionar(mochila, onde: item.bonus > 5, limite: 1), limite: 1)\n";
        let caro_primeiro = "item = selecionar(mochila, onde: selecionar(mochila, onde: item.bonus > 5, limite: 1) and item.tipo == \"pocao\", limite: 1)\n";

        let cheap_first = run_with_bag(barato_primeiro, 200, Weakness::Elemento(Element::Fogo), Posture::Guarda, &bag);
        let expensive_first = run_with_bag(caro_primeiro, 200, Weakness::Elemento(Element::Fogo), Posture::Guarda, &bag);

        assert!(
            cheap_first.cycles_used < expensive_first.cycles_used,
            "barato-primeiro ({} ciclos) deveria custar menos que caro-primeiro ({} ciclos)",
            cheap_first.cycles_used,
            expensive_first.cycles_used
        );

        // mesmo resultado nas duas ordens: acham o mesmo item ("d", a poção)
        let (_, found_cheap) = selected_event(&cheap_first);
        let (_, found_expensive) = selected_event(&expensive_first);
        assert!(found_cheap && found_expensive, "as duas ordens precisam achar o mesmo item, so o custo deve diferir");
    }

    #[test]
    fn item_found_on_first_scan_costs_one_cycle() {
        let bag = bag_of(vec![(ItemKind::Pocao, "vida", 3), (ItemKind::Escudo, "bronze", 0), (ItemKind::Magia, "fogo", 8)]);
        let src = "item = selecionar(mochila, onde: item.tipo == \"pocao\", limite: 1)\n";
        let r = run_with_bag(src, 100, Weakness::Elemento(Element::Fogo), Posture::Guarda, &bag);
        // RFC-024: 1 `Stmt` (a atribuicao) -> +1*STMT_SIZE_COST de tamanho,
        // somado ao custo de varredura.
        assert_eq!(r.cycles_used, api::SELECT_SCAN_COST + api::STMT_SIZE_COST);
        let (examined, found) = selected_event(&r);
        assert_eq!(examined, 1);
        assert!(found);
    }

    #[test]
    fn item_not_found_after_scanning_whole_bag_costs_bag_length_cycles() {
        let bag = bag_of(vec![(ItemKind::Pocao, "vida", 3), (ItemKind::Escudo, "bronze", 0), (ItemKind::Magia, "fogo", 8)]);
        let src = "item = selecionar(mochila, onde: item.tipo == \"amuleto\", limite: 1)\n";
        let r = run_with_bag(src, 100, Weakness::Elemento(Element::Fogo), Posture::Guarda, &bag);
        // RFC-024: 1 `Stmt` (a atribuicao) -> +1*STMT_SIZE_COST de tamanho.
        assert_eq!(r.cycles_used, api::SELECT_SCAN_COST * bag.0.len() as u32 + api::STMT_SIZE_COST);
        let (examined, found) = selected_event(&r);
        assert_eq!(examined, bag.0.len());
        assert!(!found);
    }

    #[test]
    fn limite_different_from_one_is_a_clear_execution_error_on_the_line() {
        let bag = bag_of(vec![(ItemKind::Pocao, "vida", 3)]);
        let program = parse("item = selecionar(mochila, onde: item.tipo == \"pocao\", limite: 2)\n").unwrap();
        let err = run_turn_with_bag(
            &program,
            &mut HashMap::new(),
            20,
            100,
            100,
            100,
            100,
            Posture::Guarda,
            Weakness::Elemento(Element::Fogo),
            10,
            false,
            None,
            None,
            Some(&bag),
        )
        .unwrap_err();
        assert!(err.message.contains("limite"));
        assert_eq!(err.line, 1);
    }

    #[test]
    fn selecionar_without_bag_or_with_empty_bag_returns_nil_at_zero_cost_never_error() {
        let src = "item = selecionar(mochila, onde: item.tipo == \"pocao\", limite: 1)\nesperar()\n";
        let program = parse(src).unwrap();

        // bag: None (RFC-002/RFC-015: ausencia nunca e erro)
        let mut vars_no_bag = HashMap::new();
        let r = run_turn_with_bag(&program, &mut vars_no_bag, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, None, None, None)
            .unwrap();
        // RFC-024: 2 `Stmt` (atribuicao + esperar) -> +2*STMT_SIZE_COST=4 de
        // tamanho, somados ao 1 ciclo de esperar() -- selecionar() em si
        // continua a custo zero quando nao ha mochila.
        assert_eq!(r.cycles_used, 1 + 2 * api::STMT_SIZE_COST, "sem mochila, selecionar nao pode custar ciclo - so o esperar() e o tamanho contam");
        assert_eq!(vars_no_bag.get("item"), Some(&Value::Nil));
        let (examined, found) = selected_event(&r);
        assert_eq!(examined, 0);
        assert!(!found);

        // mochila vazia
        let empty_bag = Bag::default();
        let mut vars_empty = HashMap::new();
        let r2 = run_turn_with_bag(
            &program,
            &mut vars_empty,
            20,
            100,
            100,
            100,
            100,
            Posture::Guarda,
            Weakness::Elemento(Element::Fogo),
            10,
            false,
            None,
            None,
            Some(&empty_bag),
        )
        .unwrap();
        assert_eq!(r2.cycles_used, 1 + 2 * api::STMT_SIZE_COST);
        assert_eq!(vars_empty.get("item"), Some(&Value::Nil));
    }

    #[test]
    fn selected_item_fields_are_accessible_inside_onde_and_after_assignment() {
        let bag = bag_of(vec![(ItemKind::Magia, "fogo", 8)]);
        let src = "item = selecionar(mochila, onde: item.nome == \"fogo\" and item.tipo == \"magia\" and item.bonus == 8, limite: 1)\nnome = item.nome\ntipo = item.tipo\nbonus = item.bonus\n";
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        run_turn_with_bag(&program, &mut vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, None, None, Some(&bag)).unwrap();
        assert_eq!(vars.get("nome"), Some(&Value::Str("fogo".to_string())));
        assert_eq!(vars.get("tipo"), Some(&Value::Str("magia".to_string())));
        assert_eq!(vars.get("bonus"), Some(&Value::Num(8.0)));
    }

    /// Prova a regra 5 (não-objetivo 5 da RFC): o item devolvido por
    /// `selecionar` tem `.bonus_damage` real da mochila (50, bem maior que
    /// qualquer bônus de equipamento de teste), mas usá-lo em `atacar()`
    /// resolve o bônus só pela correspondência com o `Loadout` **equipado**
    /// — nunca pelo campo `.bonus` do item da mochila.
    #[test]
    fn item_from_selecionar_used_in_atacar_resolves_bonus_from_equipped_loadout_not_bag_field() {
        let bag = bag_of(vec![(ItemKind::Magia, "fogo", 50)]);
        let src = "item = selecionar(mochila, onde: item.tipo == \"magia\", limite: 1)\natacar(item)\n";
        let program = parse(src).unwrap();

        // sem loadout equipado: bonus tem que ser zero, nao 50.
        let mut vars = HashMap::new();
        let r = run_turn_with_bag(&program, &mut vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false, None, None, Some(&bag))
            .unwrap();
        match r.events.iter().find(|e| matches!(e, TurnEvent::Attacked { .. })) {
            Some(TurnEvent::Attacked { damage, effective, .. }) => {
                assert!(*effective);
                assert_eq!(*damage, BASE_ATTACK_DAMAGE, "bonus do item da mochila (50) nao pode ser somado ao dano - so o loadout equipado conta");
            }
            other => panic!("evento inesperado: {other:?}"),
        }

        // com loadout equipado (bonus pequeno, 6): dano = base + 6, nunca + 50.
        let loadout = Loadout { magia: Some(InvItem { id: "x".into(), kind: ItemKind::Magia, name: "fogo".into(), bonus_damage: 6 }), ..Default::default() };
        let mut vars2 = HashMap::new();
        let r2 = run_turn_with_bag(
            &program,
            &mut vars2,
            20,
            100,
            100,
            100,
            100,
            Posture::Guarda,
            Weakness::Elemento(Element::Fogo),
            10,
            false,
            Some(&loadout),
            None,
            Some(&bag),
        )
        .unwrap();
        match r2.events.iter().find(|e| matches!(e, TurnEvent::Attacked { .. })) {
            Some(TurnEvent::Attacked { damage, .. }) => assert_eq!(*damage, BASE_ATTACK_DAMAGE + 6),
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    // --- RFC-018: validação ao vivo (parse + dry-run reais, substitui a
    // heurística estimate_cost que só contava 1 por LINHA de texto) ------

    #[test]
    fn probe_turn_with_bag_reports_real_cycles_not_line_count_for_a_loop() {
        // O bug original (RFC-018): a barra de ciclos contava 1 por LINHA,
        // não por iteração. `for i in 0..5: esperar()` tem 2 linhas de
        // texto, mas custa muito mais que 2 ciclos de verdade (5 checagens
        // de laço + 5 execuções do corpo).
        let src = "for i in 0..5:\n    esperar()\n";
        let program = parse(src).unwrap();
        let vars = HashMap::new();
        let p =
            probe_turn_with_bag(&program, &vars, 100, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), None, None, None).unwrap();
        assert!(!p.truncated);
        assert!(p.cycles_used > 2, "heuristica antiga contaria 2 (uma por linha); custo real de 5 iteracoes tem que ser bem maior");

        // E bate exatamente com o que a passada real gastaria — a garantia
        // central da RFC é que o número mostrado ao vivo é sempre o mesmo
        // que aparece depois de EXECUTAR, nunca uma aproximação.
        let real =
            run_turn(&program, &mut HashMap::new(), 100, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        assert_eq!(p.cycles_used, real.cycles_used);
    }

    #[test]
    fn probe_turn_with_bag_surfaces_the_real_validation_error_instead_of_masking_it() {
        // Script sintaticamente válido (parseia) mas semanticamente
        // inválido — variável não definida usada como argumento. A
        // heurística antiga (estimate_cost, só contava linha) não detectava
        // isso; a validação ao vivo tem que devolver o erro real, igual à
        // passada de verdade devolveria.
        let src = "atacar(naoexiste)\n";
        let program = parse(src).unwrap();
        let vars = HashMap::new();
        let err =
            probe_turn_with_bag(&program, &vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), None, None, None).unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn probe_turn_with_bag_over_budget_is_not_an_error_only_truncated_flag() {
        // RFC-018 regra 4: estourar orçamento durante a validação ao vivo
        // não é erro de sintaxe — só aparece na flag `truncated` (que a UI
        // usa pra colorir a barra de ciclos), sem travar a edição.
        let src = "while inimigo.vida > 0:\n    atacar(espada.Bronze)\n";
        let program = parse(src).unwrap();
        let vars = HashMap::new();
        let p =
            probe_turn_with_bag(&program, &vars, 6, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), None, None, None).unwrap();
        assert!(p.truncated);
        assert!(p.cycles_used <= 6);
    }

    #[test]
    fn probe_turn_with_bag_never_mutates_the_caller_vars_across_many_calls() {
        // RFC-018 regra 3 / risco "vazamento de player_vars real": mesmo
        // rodando a validação dezenas de vezes seguidas (o que acontece a
        // cada frame enquanto o jogador digita), o mapa do chamador não
        // muda — `probe_turn_with_bag` só lê `&HashMap`, clona por dentro
        // (mesma disciplina da RFC-010, regra 2).
        let src = "x = 1\ny = x + 1\natacar(espada.Bronze)\n";
        let program = parse(src).unwrap();
        let mut vars = HashMap::new();
        vars.insert("preexistente".to_string(), Value::Num(7.0));
        let snapshot = vars.clone();

        for _ in 0..20 {
            let _ =
                probe_turn_with_bag(&program, &vars, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), None, None, None);
        }

        assert_eq!(vars, snapshot, "varias validacoes ao vivo sem EXECUTAR nao podem alterar o vars real do jogador");
    }

    // --- RFC-021: fraquezas mais punitivas — bateria de ordenação para os
    // 3 monstros que nunca passaram por essa verificação (Múmia, Zumbi,
    // Escaravelho), mesma disciplina que a RFC-011 estabeleceu para Aker e
    // as RFC-012/017 já seguiam para Apagado/Chabti-Mor. ---

    #[test]
    fn mummy_naive_wrong_element_never_beats_correct_element_in_fewer_turns() {
        // RFC-021 regra 2 / criterio de aceite: Múmia (data.rs::mummy) nunca
        // tinha essa verificação. Vida 100, orçamento 20 (calibrados desde
        // sempre, não mudam por esta RFC — só o divisor mudou).
        //
        // Ingênua: ataca com o elemento errado (água) repetidamente, 10x em
        // 20 ciclos (10x2=20, sem sobra). Elemento nunca casa -> reducao
        // (RFC-021: /8): 10 x (BASE_ATTACK_DAMAGE=12 / 8) = 10 x 1 = 10
        // dano/turno.
        //
        // RFC-024: os dois scripts tem o mesmo tamanho (10 `Stmt` cada) --
        // somar STMT_SIZE_COST*10=20 ao orcamento compartilhado preserva
        // `remaining` (e o resultado) identico a antes desta RFC pros dois
        // lados ao mesmo tempo, sem precisar separar em dois orcamentos.
        let naive_src = "atacar(magia[\"agua\"])\n".repeat(10);
        let budget = 40;
        let mut life = 100;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 8, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (10x atacar = 20 ciclos + 20 de tamanho) nao deveria estourar o orcamento de 40");
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: mesmo script, só o item muda (fogo em vez de água) --
        // mesmo custo em ciclos, elemento sempre casa -> dano cheio:
        // 10 x BASE_ATTACK_DAMAGE(12) = 120 dano/turno.
        let correct_src = "atacar(magia[\"fogo\"])\n".repeat(10);
        let mut life = 100;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 8, false).unwrap();
            assert!(!r.truncated, "script com elemento certo nao pode estourar o orcamento calibrado de 40");
            life = r.enemy_life;
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script com elemento certo precisa vencer a Mumia dentro do orcamento calibrado");

        assert!(
            correct_turns < naive_turns,
            "a estrategia com elemento certo ({correct_turns} turnos) precisa vencer em menos turnos que o spam ingenuo ({naive_turns} turnos)"
        );
        assert!(
            naive_turns >= correct_turns * 2,
            "margem fraca: ingenua {naive_turns} turnos vs correta {correct_turns} turnos -- nao pode ser um empate raso"
        );
    }

    #[test]
    fn zombie_naive_waste_never_beats_efficient_script_in_fewer_turns() {
        // RFC-021 regra 2 / criterio de aceite: Zumbi (data.rs::zombie)
        // nunca tinha essa verificação -- e não podia: com
        // `cycle_budget == max_ciclos` (8 == 8, valor pré-RFC-021), nenhum
        // turno legal (que não estoure o próprio orçamento) conseguia
        // jamais ultrapassar `max_ciclos`, então `Weakness::Eficiencia`
        // nunca acertava a redução de verdade em jogo. Este teste é o que
        // expôs isso -- por isso `cycle_budget` subiu para 16 (o dobro;
        // `max_ciclos` continua 8, a condição da fraqueza não mudou, ver
        // comentário em `data.rs::zombie`). Vida 80 (inalterada).
        //
        // Ingênua: um script perdulário -- 8x esperar() (puro enchimento,
        // 8 ciclos) antes de atacar, representando um jogador que não pensa
        // em eficiência (exatamente o que Zumbi pune). cycles_used chega a
        // 8 só de enchimento; cada atacar() subsequente carrega +2 e já
        // ultrapassa max_ciclos=8 ANTES do dano ser calculado (charge()
        // roda antes de resolve_attack em eval_native_call) -> os 4 ataques
        // que cabem nos 8 ciclos restantes (4x2=8, total 16=orçamento, sem
        // sobra) saem todos reduzidos (RFC-021: /8):
        // 4 x (BASE_ATTACK_DAMAGE=12 / 8) = 4 x 1 = 4 dano/turno.
        //
        // RFC-024: `Weakness::Eficiencia` compara ciclos de *execução* --
        // `resolve_attack_by_weakness` subtrai `size_charge` antes de
        // comparar com `max_ciclos` (ver doc comment do campo em `Vm`), pelo
        // motivo de que o custo de tamanho mede o texto escrito, um eixo que
        // essa fraqueza nunca avaliou. O calculo de efetividade acima
        // continua valendo sem alteração. Só o orçamento (que cobre
        // truncamento, não efetividade) precisa somar o custo de tamanho de
        // cada script, em orçamentos separados (tamanhos diferentes):
        // ingênuo 12 `Stmt` -> +24; 16 -> 40.
        let naive_src = "esperar()\n".repeat(8) + &"atacar(espada.Bronze)\n".repeat(4);
        let naive_budget = 40;
        let mut life = 80;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), naive_budget, 100, 100, life, 80, Posture::Guarda, Weakness::Eficiencia { max_ciclos: 8 }, 6, false).unwrap();
            assert!(!r.truncated, "spam perdulario (8 esperar + 4 atacar = 16 ciclos + 24 de tamanho) nao deveria estourar o orcamento de 40");
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0, "script perdulario precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: ataca direto, sem enchimento -- 4 atacar() (8 ciclos).
        // cycles_used de execução nunca passa de 8 (o proprio custo do
        // ultimo ataque encosta exatamente no limite, "<=" inclui a borda)
        // -> dano cheio em todos: 4 x BASE_ATTACK_DAMAGE(12) = 48, mais o
        // bonus dos ciclos sobrando. Tamanho: 4 `Stmt` -> +8; 16 -> 24.
        let correct_src = "atacar(espada.Bronze)\n".repeat(4);
        let correct_budget = 24;
        let mut life = 80;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), correct_budget, 100, 100, life, 80, Posture::Guarda, Weakness::Eficiencia { max_ciclos: 8 }, 6, false).unwrap();
            assert!(!r.truncated, "script eficiente (4x atacar = 8 ciclos + 8 de tamanho) nao pode estourar o orcamento calibrado de 24");
            life = r.enemy_life;
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script eficiente precisa vencer o Zumbi dentro do orcamento calibrado");

        assert!(
            correct_turns < naive_turns,
            "a estrategia eficiente ({correct_turns} turnos) precisa vencer em menos turnos que o spam perdulario ({naive_turns} turnos)"
        );
        assert!(
            naive_turns >= correct_turns * 2,
            "margem fraca: perdularia {naive_turns} turnos vs eficiente {correct_turns} turnos -- nao pode ser um empate raso"
        );
    }

    #[test]
    fn beetle_naive_spam_never_beats_posture_branch_in_fewer_turns() {
        // RFC-021 regra 2 / criterio de aceite: Escaravelho (data.rs::beetle)
        // nunca tinha essa verificação. `ExigeGuarda` é a única fraqueza com
        // condição *ambiente* (a postura alterna sozinha a cada turno,
        // `Posture::toggled`) em vez de exigir uma ação do jogador -- um
        // script cego (sem `if`) já acerta dano cheio em metade dos turnos
        // (os de guarda), de graça. Isso bate um teto estrutural em quanto
        // qualquer divisor consegue punir aqui (ver comentário em
        // `resolve_attack_by_weakness` e em `data.rs::beetle`): com o
        // `cycle_budget` par original (16), o único ciclo do `if` de
        // bifurcação custava um ataque inteiro no turno de guarda (o de
        // maior valor), tornando o spam cego competitivo ou até melhor --
        // um antijogo real, achado por este teste. `cycle_budget` subiu
        // para 17 (ímpar: a folga absorve o custo do `if` sem custar um
        // ataque) e `max_life` para 110 (a vantagem real do script correto
        // precisa de mais de 1 turno pra aparecer). Nenhuma mudança na
        // condição da fraqueza em si.
        //
        // Ingênua: atacar() cabe 8x em 17 ciclos (8x2=16, sobra 1). Postura
        // alterna a cada turno começando em guarda:
        // guarda: 8 x BASE_ATTACK_DAMAGE(12) = 96, + bonus do 1 ciclo
        //   sobrando = 97 dano.
        // aberta: 8 x (BASE_ATTACK_DAMAGE=12 / 8 = 1) = 8, + bonus 1 = 9 dano.
        //
        // RFC-024: mesma tecnica de orcamentos separados (naive/correct) que
        // as demais bases de ordenacao. Ingenua: 8 `Stmt` -> +16; 17 -> 33.
        let naive_src = "atacar(espada.Bronze)\n".repeat(8);
        let naive_budget = 33;
        let mut life: i32 = 110;
        let mut posture = Posture::Guarda;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), naive_budget, 100, 100, life, 110, posture, Weakness::ExigeGuarda, 7, false).unwrap();
            assert!(!r.truncated, "spam cego (8x atacar = 16 de 17 ciclos + 16 de tamanho) nao deveria estourar o orcamento");
            life = r.enemy_life;
            posture = posture.toggled();
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam cego precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: lê a postura -- ataca 8x só na guarda (if custa 1 ciclo
        // + 8x2=16 = 17, exatamente o orçamento, mesmos 8 ataques da
        // ingênua: a folga ímpar absorve o `if` sem custar um ataque); na
        // aberta não ataca (sem `else`, o `if` custa só 1 ciclo) e banca os
        // 16 ciclos restantes como golpe bônus, mais valioso por ciclo do
        // que um ataque reduzido (1 dano / 2 ciclos):
        // guarda: 8 x 12 = 96 dano (sem sobra, sem bonus).
        // aberta: bonus de 17-1=16 dano. Tamanho: if(1) + 8 atacar = 9
        // `Stmt` -> +18; 17 -> 35.
        let correct_src = "if inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(8);
        let correct_budget = 35;
        let mut life: i32 = 110;
        let mut posture = Posture::Guarda;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), correct_budget, 100, 100, life, 110, posture, Weakness::ExigeGuarda, 7, false).unwrap();
            assert!(!r.truncated, "script com if de postura nao pode estourar o orcamento calibrado de 35");
            life = r.enemy_life;
            posture = posture.toggled();
            correct_turns += 1;
        }
        assert_eq!(life, 0, "script com if de postura precisa vencer o Escaravelho dentro do orcamento calibrado");

        // RFC-021: a margem aqui é estruturalmente mais modesta que nas
        // outras 5 fraquezas (todas action-gated: falham 100% das vezes
        // sem a ação certa) -- ExigeGuarda já acerta cheio de graça na
        // metade dos turnos. "menos turnos, com margem clara" continua
        // valendo (33% menos turnos com os números calibrados acima), só
        // não é o mesmo múltiplo de 2x das outras 5.
        assert!(
            correct_turns < naive_turns,
            "a estrategia com if de postura ({correct_turns} turnos) precisa vencer em menos turnos que o spam cego ({naive_turns} turnos)"
        );
    }

    // --- RFC-022: ritmo de combate — bateria permanente de 7 testes, um
    // por monstro. O playtest gravado reportou "combate entediante" e a
    // medição do product-manager achou a causa: Aker levava 15 turnos
    // (orçamento 10 só cabia 1 atacar() por turno) e não havia curva de
    // dificuldade (o 4º monstro caía em 2 turnos, o 3º levava 5). Esta
    // bateria é a mesma disciplina de guarda permanente que a RFC-011/021
    // estabeleceram para antijogo, agora aplicada a ritmo: cada teste roda
    // a estratégia correta do seu monstro, turno a turno, contra os
    // números reais de `data.rs` (nunca hardcoded — se uma RFC futura
    // recalibrar o bestiário e reabrir um atoleiro ou um duelo trivial, é
    // aqui que o CI acusa). Critério de aceite: `3..=6` turnos, faixa (não
    // número exato — RFC-022, "Decisões tomadas") para sobreviver a ajuste
    // fino futuro sem quebrar a cada tweak de dano. ---

    /// Roda a estratégia correta de um monstro turno a turno contra o
    /// `MonsterSpec` real (`crate::monsters::data`), alternando postura a
    /// cada turno quando `toggle_posture` é verdadeiro (fraquezas cuja
    /// condição depende dela: `ExigeGuarda`, `DuploSelo`), e devolve quantos
    /// turnos levou pra zerar a vida. Mesmo padrão de loop dos testes de
    /// ordenação da RFC-021 acima, só que lendo `spec.max_life`/
    /// `spec.cycle_budget`/`spec.weakness`/`spec.base_damage` de verdade em
    /// vez de duplicar os números inline — é isso que torna o teste uma
    /// guarda contra recalibração futura, não só uma default fixture.
    fn turns_to_defeat_with_spec(src: &str, spec: &MonsterSpec, toggle_posture: bool) -> u32 {
        let mut life = spec.max_life;
        let mut posture = Posture::Guarda;
        let mut turns = 0;
        while life > 0 && turns < 200 {
            let program = parse(src).unwrap();
            let r = run_turn(
                &program,
                &mut HashMap::new(),
                spec.cycle_budget,
                100,
                100,
                life,
                spec.max_life,
                posture,
                spec.weakness,
                spec.base_damage,
                false,
            )
            .unwrap();
            assert!(!r.truncated, "estrategia correta de {} nao pode estourar o orcamento calibrado ({} ciclos)", spec.title, spec.cycle_budget);
            life = r.enemy_life;
            if toggle_posture {
                posture = posture.toggled();
            }
            turns += 1;
        }
        assert_eq!(life, 0, "estrategia correta precisa vencer {} dentro do orcamento calibrado", spec.title);
        turns
    }

    #[test]
    fn mummy_rhythm_within_target_range() {
        // 1º da progressão, turno-alvo 3 (RFC-022). Correta: 3x
        // atacar(magia.Fogo) — cabe exato no orçamento calibrado (6 = 3x2).
        let spec = data::mummy();
        let src = "atacar(magia.Fogo)\n".repeat(3);
        let turns = turns_to_defeat_with_spec(&src, &spec, false);
        assert!((3..=6).contains(&turns), "Mumia deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn zombie_rhythm_within_target_range() {
        // 2º da progressão, turno-alvo 3 (RFC-022). Correta: 3x
        // atacar(espada.Ferro) — 6 ciclos, bem abaixo de max_ciclos=8,
        // então nunca aciona a redução de Eficiencia.
        let spec = data::zombie();
        let src = "atacar(espada.Ferro)\n".repeat(3);
        let turns = turns_to_defeat_with_spec(&src, &spec, false);
        assert!((3..=6).contains(&turns), "Zumbi deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn beetle_rhythm_within_target_range() {
        // 3º da progressão, turno-alvo 4 (RFC-022). Correta: lê a postura,
        // ataca 5x só na guarda (if custa 1 ciclo + 5x2=10 = 11, exatamente
        // o orçamento calibrado); na aberta banca o orçamento inteiro como
        // golpe bônus.
        let spec = data::beetle();
        let src = "if inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(5);
        let turns = turns_to_defeat_with_spec(&src, &spec, true);
        assert!((3..=6).contains(&turns), "Escaravelho deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn sphinx_rhythm_within_target_range() {
        // 4º da progressão, turno-alvo 4 (RFC-022). Correta: inspecionar()
        // sempre, seguido de 3x atacar() — 3+3x2=9, exatamente o orçamento
        // calibrado. RequerInspecao bloqueia 100% do dano sem inspecionar,
        // então não há estratégia "ingênua" competitiva a comparar aqui.
        let spec = data::sphinx();
        let src = "inspecionar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(3);
        let turns = turns_to_defeat_with_spec(&src, &spec, false);
        assert!((3..=6).contains(&turns), "Esfinge deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn guardiao_rhythm_within_target_range() {
        // 5º da progressão (Aker), turno-alvo 5 (RFC-022) — o atoleiro de
        // 15 turnos que motivou esta RFC. Correta: inspecionar() + if
        // guarda, com uma sequência real de 4 ataques (não só 1) no turno
        // de guarda — inspecionar(3) + if(1) + 4x atacar(2) = 12, exatamente
        // o orçamento calibrado (10 -> 12, regra 2 da RFC: orçamento sobe,
        // vida não muda).
        let spec = data::guardiao();
        let src = "inspecionar()\nif inimigo.postura == \"guarda\":\n".to_string()
            + &"    atacar(espada.Bronze)\n".repeat(4)
            + "else:\n    esperar()\n";
        let turns = turns_to_defeat_with_spec(&src, &spec, true);
        assert!((3..=6).contains(&turns), "Aker deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn sentinela_rhythm_within_target_range() {
        // 6º da progressão (Apagado), turno-alvo 5 (RFC-022). Correta: uma
        // func nomeada com um atacar() dentro, chamada 3x — cada chamada
        // custa USER_CALL_COST(1) + atacar(2) = 3, total 9, exatamente o
        // orçamento calibrado.
        let spec = data::sentinela();
        let src = "func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(3);
        let turns = turns_to_defeat_with_spec(&src, &spec, false);
        assert!((3..=6).contains(&turns), "Apagado deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn necroguardiao_rhythm_within_target_range() {
        // 7º e último da progressão (Chabti-Mor), turno-alvo 6 — o mais
        // longo, fechando a curva crescente (RFC-022). Correta: 2x invocar
        // (2*INVOKE_COST=4) + 2x atacar (4) = 8, exatamente o orçamento
        // calibrado.
        let spec = data::necroguardiao();
        let src = "invocar a:\n    esperar()\ninvocar b:\n    esperar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(2);
        let turns = turns_to_defeat_with_spec(&src, &spec, false);
        assert!((3..=6).contains(&turns), "Chabti-Mor deveria durar 3..=6 turnos com a estrategia correta, levou {turns}");
    }

    #[test]
    fn rhythm_curve_is_non_decreasing_across_the_seven_phases() {
        // RFC-022 critério de aceite: a curva de turnos precisa ser
        // crescente ou plana ao longo das 7 fases, na ordem de introdução
        // (`monsters::PHASES`, RFC-005 — nunca reordenada por dificuldade).
        // Reusa exatamente os mesmos 7 scripts de referência dos testes
        // acima para não divergir da definição de "estratégia correta" de
        // cada monstro.
        let mummy_turns = turns_to_defeat_with_spec(&"atacar(magia.Fogo)\n".repeat(3), &data::mummy(), false);
        let zombie_turns = turns_to_defeat_with_spec(&"atacar(espada.Ferro)\n".repeat(3), &data::zombie(), false);
        let beetle_turns = turns_to_defeat_with_spec(
            &("if inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(5)),
            &data::beetle(),
            true,
        );
        let sphinx_turns =
            turns_to_defeat_with_spec(&("inspecionar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(3)), &data::sphinx(), false);
        let guardiao_turns = turns_to_defeat_with_spec(
            &("inspecionar()\nif inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(4) + "else:\n    esperar()\n"),
            &data::guardiao(),
            true,
        );
        let sentinela_turns =
            turns_to_defeat_with_spec(&("func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(3)), &data::sentinela(), false);
        let necroguardiao_turns = turns_to_defeat_with_spec(
            &("invocar a:\n    esperar()\ninvocar b:\n    esperar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(2)),
            &data::necroguardiao(),
            false,
        );

        let curve = [mummy_turns, zombie_turns, beetle_turns, sphinx_turns, guardiao_turns, sentinela_turns, necroguardiao_turns];
        for pair in curve.windows(2) {
            assert!(
                pair[0] <= pair[1],
                "curva de ritmo precisa ser crescente ou plana entre fases consecutivas, achei {:?} (fase anterior {} > fase seguinte {})",
                curve,
                pair[0],
                pair[1]
            );
        }
    }

    // --- RFC-024: custo por instrução escrita --------------------------

    /// O teste mais importante da RFC-024 (regra 6): prova, rodando a VM de
    /// verdade (não só a conta em texto do doc comment de
    /// `api::STMT_SIZE_COST`), que existe um ponto de virada real entre
    /// desenrolar `N` ataques e escrever um `for` de `N` iterações com o
    /// mesmo corpo — desenrolado ganha (custa menos ciclos) para `N` pequeno,
    /// laço ganha para `N` grande, e os dois causam exatamente o mesmo dano
    /// (mesmo elemento, mesmo item, mesmo número de ataques efetivos) em
    /// qualquer `N`. Sem essa segunda parte a comparação seria vazia — um
    /// script mais barato que também causa menos dano não prova nada sobre
    /// "o algoritmo certo ganha".
    ///
    /// Fórmulas (ver doc comment de `STMT_SIZE_COST`): desenrolado
    /// `(STMT_SIZE_COST + 2) * N` = `4N`; laço
    /// `STMT_SIZE_COST * 2 + (LOOP_TICK_COST + 2) * N` = `4 + 3N`. Ponto de
    /// virada entre `N=4` (empate, `16 == 16`) e `N=5` (laço passa a
    /// vencer, `19 < 20`).
    #[test]
    fn unrolled_wins_small_n_loop_wins_large_n_same_damage() {
        // orçamento generoso o bastante pra nenhum dos dois lados (mesmo o
        // laço em N=10, 34 ciclos) truncar — o que importa aqui é comparar
        // `cycles_used`, não simular um duelo real contra um monstro.
        let budget = 200;
        let enemy_life = 1_000_000;

        for (n, expected) in [
            (1u32, std::cmp::Ordering::Less),
            (3, std::cmp::Ordering::Less),
            (4, std::cmp::Ordering::Equal),
            (5, std::cmp::Ordering::Greater),
            (10, std::cmp::Ordering::Greater),
        ] {
            let unrolled_src = "atacar(magia.Fogo)\n".repeat(n as usize);
            let loop_src = format!("for i in 0..{n}:\n    atacar(magia.Fogo)\n");

            let unrolled_program = parse(&unrolled_src).unwrap();
            let loop_program = parse(&loop_src).unwrap();

            let unrolled = run_turn(
                &unrolled_program,
                &mut HashMap::new(),
                budget,
                100,
                100,
                enemy_life,
                enemy_life,
                Posture::Guarda,
                Weakness::Elemento(Element::Fogo),
                10,
                false,
            )
            .unwrap();
            let looped = run_turn(
                &loop_program,
                &mut HashMap::new(),
                budget,
                100,
                100,
                enemy_life,
                enemy_life,
                Posture::Guarda,
                Weakness::Elemento(Element::Fogo),
                10,
                false,
            )
            .unwrap();

            assert!(!unrolled.truncated, "N={n}: desenrolado nao pode estourar o orcamento generoso de teste");
            assert!(!looped.truncated, "N={n}: laco nao pode estourar o orcamento generoso de teste");

            assert_eq!(
                unrolled.cycles_used.cmp(&looped.cycles_used),
                expected,
                "N={n}: esperava {:?} comparando desenrolado ({} ciclos) com laco ({} ciclos)",
                expected,
                unrolled.cycles_used,
                looped.cycles_used
            );

            // mesmo dano: os dois scripts atacam o elemento certo N vezes,
            // cada ataque efetivo e com o mesmo dano base -- a diferenca de
            // custo acima nao vem de fazer menos dano, so de escrever menos
            // (ou mais) Stmt/ciclo de execucao.
            let effective_hits = |r: &TurnResult| -> Vec<(bool, i32)> {
                r.events
                    .iter()
                    .filter_map(|e| match e {
                        TurnEvent::Attacked { effective, damage, .. } => Some((*effective, *damage)),
                        _ => None,
                    })
                    .collect()
            };
            let unrolled_hits = effective_hits(&unrolled);
            let looped_hits = effective_hits(&looped);
            assert_eq!(unrolled_hits.len(), n as usize, "N={n}: desenrolado precisa ter atacado exatamente N vezes");
            assert_eq!(looped_hits, unrolled_hits, "N={n}: laco precisa causar exatamente o mesmo dano, ataque a ataque, que o desenrolado");
            assert!(
                unrolled_hits.iter().all(|(effective, damage)| *effective && *damage == BASE_ATTACK_DAMAGE),
                "N={n}: todo ataque precisa ser efetivo com dano cheio (mesmo elemento em ambos os scripts): {unrolled_hits:?}"
            );
        }
    }

    // --- RFC-025: risco real e desgaste ---------------------------------

    /// Critério de aceite explícito da regra 3: truncar o orçamento
    /// precisa ser **sempre** estritamente pior que terminar o script, nas
    /// quatro combinações que importam (bloqueado por `defender()` ou não,
    /// carga cheia/especial ou não). Cada par compara um script de 1
    /// ataque que não trunca contra um `while` infinito que trunca sob a
    /// mesma condição de bloqueio/especial — só a variável de truncamento
    /// muda entre os dois.
    #[test]
    fn truncating_is_always_strictly_worse_than_not_truncating() {
        for (defend, special) in [(false, false), (false, true), (true, false), (true, true)] {
            let prefix = if defend { "defender(escudo.Bronze)\n" } else { "" };
            let normal_src = format!("{prefix}atacar(espada.Ferro)\n");
            let trunc_src = format!("{prefix}while inimigo.vida > 0:\n    atacar(espada.Ferro)\n");

            let normal_program = parse(&normal_src).unwrap();
            let trunc_program = parse(&trunc_src).unwrap();

            // orcamento pequeno o bastante pro while nunca terminar (vida
            // do inimigo enorme de proposito) mas generoso o bastante pro
            // script de 1 ataque nunca truncar.
            let budget = 20;
            let normal = run_turn(
                &normal_program,
                &mut HashMap::new(),
                budget,
                100,
                100,
                1_000_000,
                1_000_000,
                Posture::Guarda,
                Weakness::ExigeGuarda,
                10,
                special,
            )
            .unwrap();
            let trunc = run_turn(
                &trunc_program,
                &mut HashMap::new(),
                budget,
                100,
                100,
                1_000_000,
                1_000_000,
                Posture::Guarda,
                Weakness::ExigeGuarda,
                10,
                special,
            )
            .unwrap();

            assert!(!normal.truncated, "script de 1 ataque nao pode truncar (defend={defend}, special={special})");
            assert!(trunc.truncated, "while infinito precisa truncar (defend={defend}, special={special})");
            assert!(
                trunc.player_life < normal.player_life,
                "truncar precisa causar estritamente mais dano que nao truncar (defend={defend}, special={special}): truncado sobrou {} de vida, normal sobrou {}",
                trunc.player_life,
                normal.player_life
            );
        }
    }

    /// Simula um duelo completo turno a turno contra o `MonsterSpec` real,
    /// aplicando a mesma progressão de carga que `MonsterState`
    /// (`monsters/mod.rs`) usa de verdade — `+CHARGE_PER_TURN` por turno,
    /// especial quando a carga atinge `CHARGE_THRESHOLD`, carga zera
    /// depois de um golpe especial acontecer. Para no que vier primeiro: o
    /// monstro morre (`Some(turnos)`) ou o jogador morre (`None`).
    /// `player_life` é `&mut`: os dois testes de campanha abaixo encadeiam
    /// chamadas entre fases para simular a expedição inteira, exatamente
    /// como `PhaseScene`/`SaveData::player_life` fazem de verdade
    /// (`scenes/phase.rs`, `inventory.rs`), só que sem depender de save em
    /// disco.
    fn simulate_phase(src: &str, spec: &MonsterSpec, toggle_posture: bool, player_life: &mut i32) -> Option<u32> {
        let mut life = spec.max_life;
        let mut posture = Posture::Guarda;
        let mut charge = 0u32;
        let mut turns = 0u32;
        while life > 0 && *player_life > 0 && turns < 200 {
            charge += crate::monsters::CHARGE_PER_TURN;
            let special_ready = charge >= crate::monsters::CHARGE_THRESHOLD;
            let program = parse(src).unwrap();
            let r = run_turn(
                &program,
                &mut HashMap::new(),
                spec.cycle_budget,
                *player_life,
                100,
                life,
                spec.max_life,
                posture,
                spec.weakness,
                spec.base_damage,
                special_ready,
            )
            .unwrap();
            life = r.enemy_life;
            *player_life = r.player_life;
            if r.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { special: true, .. })) {
                charge = 0;
            }
            if toggle_posture {
                posture = posture.toggled();
            }
            turns += 1;
        }
        if *player_life <= 0 {
            None
        } else if life <= 0 {
            Some(turns)
        } else {
            // nem o monstro nem o jogador morreram em 200 turnos -- nao
            // deveria acontecer com nenhum dos scripts usados abaixo
            // (calibrados pra vencer ou pra travar em RequerInspecao).
            None
        }
    }

    /// RFC-025 regra 8, critério de aceite mais importante da RFC: simula
    /// as 7 fases em sequência com a mesma estratégia correta de cada
    /// monstro usada nos testes de ritmo da RFC-022 (`*_rhythm_within_
    /// target_range` acima), a vida do jogador atravessando as fases com a
    /// recuperação parcial da regra 6
    /// (`inventory::recovered_player_life`, 90%), e confirma que o
    /// jogador chega vivo ao fim. Não usa `curar()` em nenhum script: os 7
    /// orçamentos calibrados pela RFC-024 não sobram ciclo pra isso na
    /// maioria dos monstros (ver `RFC-025-entrega-gamedev.md`) — a prova
    /// de que a campanha é sobrevivível vem só da recuperação entre fases,
    /// que é exatamente o que este teste calibra.
    #[test]
    fn campanha_bem_jogada_sobrevive() {
        let phases: [(String, MonsterSpec, bool); 7] = [
            ("atacar(magia.Fogo)\n".repeat(3), data::mummy(), false),
            ("atacar(espada.Ferro)\n".repeat(3), data::zombie(), false),
            ("if inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(5), data::beetle(), true),
            ("inspecionar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(3), data::sphinx(), false),
            (
                "inspecionar()\nif inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(4) + "else:\n    esperar()\n",
                data::guardiao(),
                true,
            ),
            ("func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(3), data::sentinela(), false),
            (
                "invocar a:\n    esperar()\ninvocar b:\n    esperar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(2),
                data::necroguardiao(),
                false,
            ),
        ];

        let mut player_life = 100i32;
        for (i, (src, spec, toggle_posture)) in phases.iter().enumerate() {
            // regra 6: recuperacao parcial antes de cada fase (menos a
            // primeira, onde a vida ja comeca cheia e a formula e no-op).
            player_life = crate::inventory::recovered_player_life(player_life, 100);
            let turns = simulate_phase(src, spec, *toggle_posture, &mut player_life);
            assert!(
                turns.is_some(),
                "campanha bem jogada precisa vencer a fase {} ({}) sem o jogador morrer -- vida ficou em {}",
                i + 1,
                spec.title,
                player_life
            );
        }
        assert!(player_life > 0, "campanha bem jogada precisa terminar as 7 fases com o jogador vivo, terminou com {player_life}");
    }

    /// RFC-025 regra 8, o par do teste acima: joga mal — ignora a fraqueza
    /// de cada monstro (elemento errado, sem ler postura, sem inspecionar)
    /// — e confirma que o jogador morre antes de terminar a campanha. Não
    /// precisa ir muito longe: a Múmia sozinha, atacada com o elemento
    /// errado, leva 25 turnos pra cair (dano reduzido /8) — e com o
    /// monstro atacando todo turno (regra 1), 25 turnos de acúmulo já
    /// bastam pra matar o jogador dentro da primeira fase. É exatamente o
    /// resultado que prova a tese da RFC: jogar mal agora tem custo real,
    /// não só "demora mais".
    #[test]
    fn campanha_mal_jogada_morre() {
        let naive_phases: [(String, MonsterSpec, bool); 7] = [
            ("atacar(magia.Agua)\n".repeat(4), data::mummy(), false),
            ("atacar(espada.Ferro)\n".repeat(4), data::zombie(), false),
            ("atacar(espada.Bronze)\n".repeat(5), data::beetle(), true),
            ("atacar(espada.Bronze)\n".repeat(4), data::sphinx(), false),
            ("atacar(espada.Bronze)\n".repeat(6), data::guardiao(), true),
            ("atacar(espada.Bronze)\n".repeat(4), data::sentinela(), false),
            ("atacar(espada.Bronze)\n".repeat(4), data::necroguardiao(), false),
        ];

        let mut player_life = 100i32;
        let mut died = false;
        for (src, spec, toggle_posture) in naive_phases.iter() {
            player_life = crate::inventory::recovered_player_life(player_life, 100);
            if simulate_phase(src, spec, *toggle_posture, &mut player_life).is_none() {
                died = true;
                break;
            }
        }
        assert!(died, "campanha mal jogada (ignorando as fraquezas) precisa matar o jogador antes do fim das 7 fases, mas ele chegou vivo");
    }
}



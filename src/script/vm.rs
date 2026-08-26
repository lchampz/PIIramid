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
    CounterAttack { damage: i32, blocked: bool, special: bool },
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

    shielded: bool,

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
    // antes de chegar lá de verdade na passada de verdade.
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
        funcs.clone(),
        true,
        loadout,
        player_class,
        bag,
    );
    match probe.exec_program(program) {
        Ok(()) => {}
        Err(Signal::Truncated { .. }) => {} // ok: a segunda passada vai truncar do mesmo jeito
        Err(Signal::Error(e)) => return Err(e),
    }

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

    if truncated {
        let blocked = vm.shielded;
        let base = if enemy_special_ready { enemy_base_damage * 5 / 2 } else { enemy_base_damage };
        let dmg = if blocked { base / 2 } else { base };
        vm.player_life = (vm.player_life - dmg).max(0);
        vm.events.push(TurnEvent::CounterAttack { damage: dmg, blocked, special: enemy_special_ready });
    } else {
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
            shielded: false,
            loadout,
            player_class,
            bag,
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
                    self.shielded = true;
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

    fn resolve_attack_by_weakness(&self, item: &Item) -> (i32, bool) {
        match self.enemy_weakness {
            Weakness::Elemento(elem) => {
                if Element::from_name(&item.name) == elem {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 3, false)
                }
            }
            Weakness::Eficiencia { max_ciclos } => {
                if self.cycles_used <= max_ciclos {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 4, false)
                }
            }
            Weakness::ExigeGuarda => {
                if self.enemy_posture == Posture::Guarda {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 4, false)
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
            // calibrado (não chutado) pelo teste de ordenação
            // `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns`
            // em vez de copiado de outra fraqueza: /4 faz a estrategia
            // ingenua (atacar() repetido no nivel superior) perder em
            // turnos com margem clara, nunca empatar raso, contra a
            // estrategia correta (mesmo atacar(), de dentro de uma func).
            Weakness::ExigeNomeacao => {
                if self.depth > 0 {
                    (BASE_ATTACK_DAMAGE, true)
                } else {
                    (BASE_ATTACK_DAMAGE / 4, false)
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
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 3);
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
        // sem contra-ataque: vida do jogador intacta
        assert_eq!(r.player_life, 100);
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
        let src = "func combo():\n    atacar(espada[\"ferro\"])\n    defender(escudo[\"ouro\"])\n\ncombo()\n";
        let r = run(src, 20, Weakness::Elemento(Element::Fogo), Posture::Guarda);
        assert_eq!(r.cycles_used, api::USER_CALL_COST + 2 + 1);
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
        // Escaravelho: vida 90, orcamento 16, fraqueza ExigeGuarda
        // (src/monsters/data.rs). O combo so ataca quando a postura
        // permite; sem esse `if` o script nao vence (harness.md: "um
        // script sem if nao vence"). A versao com func e a equivalente
        // sem func tomam exatamente a mesma decisao e acertam o mesmo
        // golpe efetivo — mas a versao com func gasta 1 ciclo mais
        // (USER_CALL_COST) e por isso sobra 1 ciclo menos pro golpe
        // bonus no fim do turno (vm.rs: `remaining` vira dano extra). O
        // resultado final da vida do inimigo difere em exatamente 1 ponto
        // de vida: a abstracao nunca e mais barata que o inline, nem por
        // acidente via o golpe bonus.
        let with_func = "func combo():\n    if inimigo.postura == \"guarda\":\n        atacar(espada[\"ferro\"])\n    else:\n        esperar()\n\ncombo()\n";
        let without_func = "if inimigo.postura == \"guarda\":\n    atacar(espada[\"ferro\"])\nelse:\n    esperar()\n";

        let budget = 16;
        let with = run(with_func, budget, Weakness::ExigeGuarda, Posture::Guarda);
        let without = run(without_func, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!with.truncated);
        assert!(!without.truncated);
        assert_eq!(with.cycles_used, without.cycles_used + api::USER_CALL_COST);
        assert_eq!(with.enemy_life, without.enemy_life + api::USER_CALL_COST as i32);

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
        // Orcamento calibrado (Guardiao, data.rs::guardiao()) = 10: cabe o
        // pior caso com folga pequena (4 ciclos de bonus), sem sobra
        // excessiva como nos outros 4 monstros (sphinx sobra 19).
        let src = "inspecionar()\nif inimigo.postura == \"guarda\":\n    atacar(espada.Bronze)\nelse:\n    esperar()\n";
        let budget = 10;
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
        let naive_src = "atacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\natacar(espada.Bronze)\n";
        let budget = 10;
        let mut life = 150;
        let mut posture = Posture::Guarda;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 150, posture, Weakness::DuploSelo, 9, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (5x atacar = 10 ciclos) nao deveria estourar o orcamento de 10");
            life = r.enemy_life;
            posture = posture.toggled();
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Estrategia correta: mesmo script de referencia da RFC-008 --
        // inspeciona sempre, ataca so na guarda, espera na aberta. So a
        // reducao de dano do braco "condicoes nao compostas" mudou; o braco
        // de sucesso (BASE_ATTACK_DAMAGE cheio) e igual ao de antes, logo o
        // resultado continua ~15 turnos, nao mudou com a correcao do /8.
        let correct_src = "inspecionar()\nif inimigo.postura == \"guarda\":\n    atacar(espada.Bronze)\nelse:\n    esperar()\n";
        let mut life = 150;
        let mut posture = Posture::Guarda;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 150, posture, Weakness::DuploSelo, 9, false).unwrap();
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
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 4);
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
        // depth==0 o turno inteiro -> cada golpe usa a reducao /4:
        // 8 x (BASE_ATTACK_DAMAGE=12 / 4) = 8 x 3 = 24 dano/turno.
        let naive_src = "atacar(espada.Bronze)\n".repeat(8);
        let budget = 16;
        let mut life = 150;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeNomeacao, 8, false).unwrap();
            assert!(!r.truncated, "spam ingenuo (8x atacar = 16 ciclos) nao deveria estourar o orcamento de 16");
            life = r.enemy_life;
            naive_turns += 1;
        }
        assert_eq!(life, 0, "spam ingenuo precisa vencer eventualmente (senao o teste nao compara nada)");

        // Correta: golpe() custa USER_CALL_COST(1) + atacar(2) = 3 ciclos
        // por invocacao; cabe 5x em 16 (5x3=15, sobra 1 -> golpe bonus de
        // fim de turno). depth>0 dentro do corpo de golpe() -> dano cheio:
        // 5 x 12 + 1 (bonus) = 61 dano/turno.
        let correct_src = "func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(5);
        let mut life = 150;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let r = run_turn(&program, &mut HashMap::new(), budget, 100, 100, life, 150, Posture::Guarda, Weakness::ExigeNomeacao, 8, false).unwrap();
            assert!(!r.truncated, "script com func nao pode estourar o orcamento calibrado de 16");
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
        let r_low = run_with_loadout(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(&low));
        let r_high = run_with_loadout(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(&high));

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
        let r = run_with_loadout(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(&loadout));
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
        let r = run_with_loadout(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(&wrong_name));
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
        let sem_classe = run_with_class(src, 2, Weakness::ExigeGuarda, Posture::Guarda, None);
        let guerreiro = run_with_class(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(PlayerClass::Guerreiro));

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
        let sem_classe = run_with_class(src, 2, Weakness::Elemento(Element::Fogo), Posture::Guarda, None);
        let mago = run_with_class(src, 2, Weakness::Elemento(Element::Fogo), Posture::Guarda, Some(PlayerClass::Mago));
        assert_eq!(100 - mago.enemy_life, (100 - sem_classe.enemy_life) + api::CLASS_BONUS_DAMAGE);
    }

    #[test]
    fn ladrao_attacking_with_pocao_deals_more_damage_than_without_class() {
        // ladrao ataca com pocao (RFC-003: afinidade tematica, nao ha
        // restricao de atacar com pocao na linguagem -- e so um ItemKind).
        let src = "atacar(pocao.Vida)\n";
        let sem_classe = run_with_class(src, 2, Weakness::ExigeGuarda, Posture::Guarda, None);
        let ladrao = run_with_class(src, 2, Weakness::ExigeGuarda, Posture::Guarda, Some(PlayerClass::Ladrao));
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
        let naive_src = "atacar(espada.Bronze)\n".repeat(8);
        let budget = 16;
        let mut life = 150;
        let mut naive_turns = 0;
        while life > 0 && naive_turns < 200 {
            let program = parse(&naive_src).unwrap();
            let mut vars = HashMap::new();
            let r = run_turn_with_loadout_and_class(
                &program,
                &mut vars,
                budget,
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
        let mut life = 150;
        let mut correct_turns = 0;
        while life > 0 && correct_turns < 200 {
            let program = parse(&correct_src).unwrap();
            let mut vars = HashMap::new();
            let r = run_turn_with_loadout_and_class(
                &program,
                &mut vars,
                budget,
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
        let sem_pocao = run_curar(4, None, None);
        let com_pocao = run_curar(4, Some(&loadout_with_potion("vida", 6)), None);

        assert_eq!(sem_pocao.player_life, 50 + HEAL_AMOUNT, "sem pocao equipada, cura deve ser exatamente HEAL_AMOUNT");
        assert_eq!(com_pocao.player_life, 50 + HEAL_AMOUNT + 6, "pocao equipada com bonus_damage deve curar mais");
        assert!(com_pocao.player_life > sem_pocao.player_life, "pocao com bonus_damage maior precisa curar mais que sem pocao equipada");
    }

    #[test]
    fn ladrao_using_curar_gets_class_bonus_guerreiro_does_not() {
        let sem_classe = run_curar(4, None, None);
        let ladrao = run_curar(4, None, Some(PlayerClass::Ladrao));
        let guerreiro = run_curar(4, None, Some(PlayerClass::Guerreiro));

        assert_eq!(ladrao.player_life, sem_classe.player_life + api::CLASS_BONUS_DAMAGE, "Ladrao usando curar() com pocao deve receber CLASS_BONUS_DAMAGE");
        assert_eq!(guerreiro.player_life, sem_classe.player_life, "afinidade do Guerreiro e Espada, nao Pocao -- curar() nao deve conceder bonus");
    }

    #[test]
    fn curar_without_item_or_class_heals_exactly_heal_amount() {
        // sem loadout e sem classe: mesmo comportamento de antes da RFC-014.
        let r = run_curar(4, None, None);
        assert_eq!(r.player_life, 50 + HEAL_AMOUNT, "sem item/classe, curar() deve curar exatamente HEAL_AMOUNT, igual ao comportamento pre-RFC-014");
    }

    // --- RFC-004: invocar (threads de invocacao do necromante) --------

    #[test]
    fn invoke_single_attack_deals_real_damage() {
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\n";
        let r = run(src, 4, Weakness::ExigeGuarda, Posture::Guarda);
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
        // orcamento principal == exatamente 2*INVOKE_COST: paga as duas
        // invocacoes e nada mais, sem sobra pra golpe bonus interferir na
        // medicao do dano.
        let budget = 2 * api::INVOKE_COST;
        let r = run(src, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!r.truncated, "duas invocacoes dentro do orcamento principal nao podem truncar o turno");
        assert!(
            !r.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { .. })),
            "sem truncamento do turno principal nao pode haver contra-ataque: {:?}",
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
        let budget = api::INVOKE_COST + 1; // so precisa cobrir invocar + esperar()
        let r = run(src, budget, Weakness::ExigeGuarda, Posture::Guarda);

        assert!(!r.truncated, "estouro de orcamento dentro de invocar nao pode truncar o turno principal");
        assert!(
            !r.events.iter().any(|e| matches!(e, TurnEvent::Truncated { .. })),
            "truncamento de invocacao nao pode gerar TurnEvent::Truncated do turno: {:?}",
            r.events
        );
        assert!(
            !r.events.iter().any(|e| matches!(e, TurnEvent::CounterAttack { .. })),
            "truncamento de invocacao nao pode disparar contra-ataque: {:?}",
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
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\n";
        let r = run(src, 4, Weakness::ExigeNomeacao, Posture::Guarda);
        match &r.events[0] {
            TurnEvent::Attacked { effective, damage, .. } => {
                assert!(!*effective, "atacar() dentro de invocar sem func interno nao pode ser efetivo contra Apagado");
                assert_eq!(*damage, BASE_ATTACK_DAMAGE / 4);
            }
            other => panic!("evento inesperado: {other:?}"),
        }
    }

    #[test]
    fn reference_invoke_script_fits_every_current_monster_budget() {
        // Jogabilidade (criterio de aceite): as duas invocacoes do exemplo
        // da RFC, combinadas com um script principal razoavel, nao podem
        // estourar o orcamento principal de nenhum monstro do bestiario
        // atual (menor orcamento: Zumbi, 8 ciclos - src/monsters/data.rs).
        let src = "invocar esqueleto:\n    atacar(espada.Ferro)\ninvocar mago_morto:\n    atacar(magia.Fogo)\natacar(espada.Ferro)\n";
        let program = parse(src).unwrap();
        // custo no orcamento principal: 2*INVOKE_COST + 1 atacar() = 4 + 2 = 6
        let r = run_turn(&program, &mut HashMap::new(), 8, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        assert!(!r.truncated, "script de referencia com duas invocacoes nao pode estourar nem o menor orcamento do bestiario (Zumbi, 8)");
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
        assert_eq!(r.cycles_used, api::SELECT_SCAN_COST);
        let (examined, found) = selected_event(&r);
        assert_eq!(examined, 1);
        assert!(found);
    }

    #[test]
    fn item_not_found_after_scanning_whole_bag_costs_bag_length_cycles() {
        let bag = bag_of(vec![(ItemKind::Pocao, "vida", 3), (ItemKind::Escudo, "bronze", 0), (ItemKind::Magia, "fogo", 8)]);
        let src = "item = selecionar(mochila, onde: item.tipo == \"amuleto\", limite: 1)\n";
        let r = run_with_bag(src, 100, Weakness::Elemento(Element::Fogo), Posture::Guarda, &bag);
        assert_eq!(r.cycles_used, api::SELECT_SCAN_COST * bag.0.len() as u32);
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
        assert_eq!(r.cycles_used, 1, "sem mochila, selecionar nao pode custar ciclo - so o esperar() conta");
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
        assert_eq!(r2.cycles_used, 1);
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
}

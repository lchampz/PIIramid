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

pub struct Vm {
    vars: HashMap<String, Value>,
    cycles_used: u32,
    cycle_budget: u32,
    dry_run: bool,
    events: Vec<TurnEvent>,

    player_life: i32,
    player_max_life: i32,

    enemy_life: i32,
    enemy_max_life: i32,
    enemy_posture: Posture,
    enemy_weakness: Weakness,
    enemy_inspected: bool,

    shielded: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run_turn(
    program: &[Stmt],
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
    // primeira passada: só valida, sem efeitos colaterais
    let mut probe = Vm::new(
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        true,
    );
    match probe.exec_program(program) {
        Ok(()) => {}
        Err(Signal::Truncated { .. }) => {} // ok: a segunda passada vai truncar do mesmo jeito
        Err(Signal::Error(e)) => return Err(e),
    }

    // segunda passada: roda de verdade
    let mut vm = Vm::new(
        cycle_budget,
        player_life,
        player_max_life,
        enemy_life,
        enemy_max_life,
        enemy_posture,
        enemy_weakness,
        false,
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
        if remaining > 0 && vm.enemy_life > 0 {
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

impl Vm {
    #[allow(clippy::too_many_arguments)]
    fn new(
        cycle_budget: u32,
        player_life: i32,
        player_max_life: i32,
        enemy_life: i32,
        enemy_max_life: i32,
        enemy_posture: Posture,
        enemy_weakness: Weakness,
        dry_run: bool,
    ) -> Self {
        Vm {
            vars: HashMap::new(),
            cycles_used: 0,
            cycle_budget,
            dry_run,
            events: Vec::new(),
            player_life,
            player_max_life,
            enemy_life,
            enemy_max_life,
            enemy_posture,
            enemy_weakness,
            enemy_inspected: false,
            shielded: false,
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
                        Ok(Value::Item(Item { kind, name }))
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
                    Value::Collection(kind) => Ok(Value::Item(Item { kind, name: field.to_lowercase() })),
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
        }
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

    fn eval_call(&mut self, name: &str, args: &[Expr], line: usize) -> VResult<Value> {
        let cost = api::call_cost(name).ok_or_else(|| self.err(line, format!("funcao desconhecida: '{name}'")))?;
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
                let _item = self.expect_item(&values, name, line)?;
                if !self.dry_run {
                    self.player_life = (self.player_life + HEAL_AMOUNT).min(self.player_max_life);
                    self.events.push(TurnEvent::Healed { line, amount: HEAL_AMOUNT });
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
            _ => Err(self.err(line, format!("funcao desconhecida: '{name}'"))),
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

    fn resolve_attack(&self, item: &Item) -> (i32, bool) {
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
    use crate::script::parser::parse;

    fn run(src: &str, budget: u32, weakness: Weakness, posture: Posture) -> TurnResult {
        let program = parse(src).unwrap();
        run_turn(&program, budget, 100, 100, 100, 100, posture, weakness, 10, false).unwrap()
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
        let normal = run_turn(&program, 6, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false).unwrap();
        let special = run_turn(&program, 6, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, true).unwrap();
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
        let err = run_turn(&program, 20, 100, 100, 100, 100, Posture::Guarda, Weakness::Elemento(Element::Fogo), 10, false);
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
}

//! Ensaio Geral (RFC-027): roda o script já parseado do editor repetidamente
//! sobre um CLONE local de `player_vars`/`MonsterState`/vida do jogador,
//! turno a turno, reaproveitando `vm::simulate_turn` — a mesma rotina que o
//! turno real (`scenes/duel.rs::run_script`) usa. Nunca recebe nem devolve
//! uma referência mutável para o estado real: `rehearse` só lê
//! (`&HashMap`, `&MonsterState`), clona internamente, e todo o resto do
//! módulo (`Vm`, `MonsterState::begin_turn`, `CHARGE_PER_TURN`,
//! `Posture::toggled`) já é o mesmo determinismo que a RFC descreve como
//! "seguro simular adiante" — não há nenhum ponto de aleatoriedade a
//! esconder aqui, só repetição.

use std::collections::HashMap;

use super::ast::Stmt;
use super::error::ScriptError;
use super::value::Value;
use super::vm;
use crate::inventory::{Bag, Loadout, PlayerClass};
use crate::monsters::MonsterState;

/// Teto de segurança (RFC-027, regra 3): generoso o bastante para qualquer
/// duelo real do bestiário atual, mas finito — um script que nunca fecha o
/// duelo (ex.: `while` que sempre trunca sem nunca causar dano líquido)
/// precisa de um fim, não pode travar a UI.
pub const REHEARSAL_TURN_CAP: usize = 50;

/// Uma linha da lista compacta por turno (RFC-027, regra 4).
#[derive(Debug, Clone, PartialEq)]
pub struct RehearsalTurn {
    pub turn: usize,
    /// Vida do inimigo perdida neste turno (sempre >= 0: nenhum comando do
    /// jogador cura o inimigo).
    pub damage_dealt: i32,
    pub cycles_used: u32,
    pub cycle_budget: u32,
    pub truncated: bool,
    /// Variação líquida da vida do jogador neste turno, com sinal invertido
    /// (positivo = perdeu vida, como "dano recebido" da RFC pede). Pode ser
    /// negativo quando `curar()` no mesmo turno compensa o contra-ataque —
    /// é informação real, não um caso a esconder atrás de um `.max(0)`.
    pub damage_taken: i32,
}

/// Por que a simulação parou.
#[derive(Debug, Clone, PartialEq)]
pub enum RehearsalEnd {
    MonsterDied,
    PlayerDied,
    TurnCapReached,
    /// Um turno simulado, no meio da campanha, encontrou um erro de
    /// execução real (não de sintaxe — isso já foi filtrado antes de chamar
    /// `rehearse`, regra 2 da RFC). Só é alcançável em tese: o script já
    /// passou pela validação de sintaxe/tipo (`parser::parse` +
    /// `probe_pass`) contra o estado do turno atual antes de chegar aqui;
    /// um erro só pode nascer de um estado *futuro* que o probe atual não
    /// viu (ex.: uma expressão que divide por `inimigo.vida` e só zera daqui
    /// a alguns turnos). Reportado em vez de descartado silenciosamente ou
    /// interrompendo o processo.
    Error(ScriptError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RehearsalReport {
    pub turns: Vec<RehearsalTurn>,
    pub end: RehearsalEnd,
}

/// Roda o Ensaio Geral. `vars`, `monster` e `player_life` são só lidos:
/// clonados na primeira linha, nunca devolvidos, nunca escritos de volta no
/// que o chamador passou. `program` já deve ter passado pelo parser (regra 2
/// da RFC — erro de sintaxe é responsabilidade do chamador, antes de chamar
/// isto).
/// `#[allow(dead_code)]`: fora dos testes (`cfg(test)`), só
/// `rehearse_with_compiled_funcs` é chamada (`scenes/duel.rs`) -- mesmo
/// motivo que `vm::run_turn`/`vm::simulate_turn` já documentam: este crate
/// é um binário, não uma lib, então `pub` não isenta o lint de dead-code.
#[allow(dead_code)]
#[allow(clippy::too_many_arguments)]
pub fn rehearse(
    program: &[Stmt],
    vars: &HashMap<String, Value>,
    player_life: i32,
    player_max_life: i32,
    monster: &MonsterState,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
) -> RehearsalReport {
    rehearse_with_compiled_funcs(program, vars, player_life, player_max_life, monster, loadout, player_class, bag, &[])
}

/// Mesmo Ensaio Geral que `rehearse`, com a lista de nomes de `func`
/// "compiladas" (RFC-030) — sem isso, o Ensaio preveria mais ciclos gastos
/// do que o turno real vai cobrar para um script que chama uma func já
/// compilada, quebrando a garantia de que ensaiar bate com executar de
/// verdade (mesmo raciocínio do critério de aceite #2 abaixo). `rehearse`
/// continua existindo só pela compatibilidade dos testes já escritos
/// contra ela, encaminhando aqui com uma lista vazia.
#[allow(clippy::too_many_arguments)]
pub fn rehearse_with_compiled_funcs(
    program: &[Stmt],
    vars: &HashMap<String, Value>,
    player_life: i32,
    player_max_life: i32,
    monster: &MonsterState,
    loadout: Option<&Loadout>,
    player_class: Option<PlayerClass>,
    bag: Option<&Bag>,
    compiled_funcs: &[String],
) -> RehearsalReport {
    let mut sim_vars = vars.clone();
    let mut sim_monster = monster.clone();
    let mut sim_player_life = player_life;
    let mut turns = Vec::with_capacity(REHEARSAL_TURN_CAP);

    for turn in 1..=REHEARSAL_TURN_CAP {
        let enemy_life_before = sim_monster.life;
        let result = match vm::simulate_turn_with_compiled_funcs(
            program,
            &mut sim_vars,
            &mut sim_monster,
            sim_player_life,
            player_max_life,
            loadout,
            player_class,
            bag,
            compiled_funcs,
        ) {
            Ok(r) => r,
            Err(e) => return RehearsalReport { turns, end: RehearsalEnd::Error(e) },
        };

        let damage_dealt = enemy_life_before - sim_monster.life;
        let damage_taken = sim_player_life - result.player_life;
        sim_player_life = result.player_life;

        turns.push(RehearsalTurn {
            turn,
            damage_dealt,
            cycles_used: result.cycles_used,
            cycle_budget: result.cycle_budget,
            truncated: result.truncated,
            damage_taken,
        });

        if !sim_monster.alive() {
            return RehearsalReport { turns, end: RehearsalEnd::MonsterDied };
        }
        if sim_player_life <= 0 {
            return RehearsalReport { turns, end: RehearsalEnd::PlayerDied };
        }
    }

    RehearsalReport { turns, end: RehearsalEnd::TurnCapReached }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monsters::data;
    use crate::script::parser;

    fn mummy_state() -> MonsterState {
        MonsterState::new(data::mummy())
    }

    /// Critério de aceite #1 (regra de ouro da RFC): ensaiar nunca mexe no
    /// que foi passado — nem `vars`, nem o `MonsterState` original (a
    /// função nem recebe `&mut`, então isto também é garantido pelo
    /// compilador; o teste é a segunda trava, bit-a-bit).
    #[test]
    fn rehearse_never_mutates_the_state_it_was_given() {
        let program = parser::parse("atacar(magia.Fogo)").unwrap();
        let vars_before: HashMap<String, Value> = HashMap::new();
        let monster_before = mummy_state();
        let player_life_before = 100;

        let vars_snapshot = vars_before.clone();
        let monster_snapshot = monster_before.clone();

        let report = rehearse(&program, &vars_before, player_life_before, 100, &monster_before, None, None, None);

        assert!(!report.turns.is_empty());
        assert_eq!(vars_before, vars_snapshot);
        assert_eq!(monster_before, monster_snapshot);
        assert_eq!(player_life_before, 100);
    }

    /// Critério de aceite #2: ensaiar o script de referência da Múmia
    /// (RFC-022, `data::mummy()`) precisa produzir o MESMO número de turnos
    /// e o MESMO resultado final que rodar esse script de verdade, turno a
    /// turno, via `vm::simulate_turn` — a prova de que a simulação não é
    /// uma segunda lógica divergente da real (ela literalmente chama a
    /// mesma função).
    #[test]
    fn rehearsing_the_mummy_matches_running_it_for_real_turn_by_turn() {
        let script = "atacar(magia.Fogo)\natacar(magia.Fogo)\natacar(magia.Fogo)";
        let program = parser::parse(script).unwrap();

        // "de verdade": mesmo laço que `scenes/duel.rs::run_script` faria a
        // cada EXECUTAR, turno a turno, sobre seu próprio estado.
        let mut real_monster = mummy_state();
        let mut real_vars: HashMap<String, Value> = HashMap::new();
        let mut real_player_life = 100;
        let mut real_turns = 0usize;
        let real_end = loop {
            real_turns += 1;
            let r = vm::simulate_turn(&program, &mut real_vars, &mut real_monster, real_player_life, 100, None, None, None).unwrap();
            real_player_life = r.player_life;
            if !real_monster.alive() {
                break "monstro morreu";
            }
            if real_player_life <= 0 {
                break "jogador morreu";
            }
            if real_turns >= REHEARSAL_TURN_CAP {
                break "teto";
            }
        };

        // "ensaiado": mesmo script, mesmo estado inicial, via `rehearse`.
        let sim_monster = mummy_state();
        let sim_vars: HashMap<String, Value> = HashMap::new();
        let report = rehearse(&program, &sim_vars, 100, 100, &sim_monster, None, None, None);

        assert_eq!(report.turns.len(), real_turns);
        assert_eq!(report.end, RehearsalEnd::MonsterDied);
        assert_eq!(real_end, "monstro morreu");
        assert_eq!(real_monster.life, 0);
    }

    /// Critério de aceite #4: um script que nunca fecha o duelo (aqui, um
    /// `while` que sempre estoura o orçamento sem nunca chamar `atacar()`,
    /// o mesmo exemplo que a RFC cita) para no teto de 50 turnos em vez de
    /// rodar para sempre.
    #[test]
    fn a_script_that_never_wins_stops_at_the_turn_cap() {
        let program = parser::parse("while inimigo.vida > -1000:\n    esperar()").unwrap();
        let monster = mummy_state();
        let vars: HashMap<String, Value> = HashMap::new();

        let report = rehearse(&program, &vars, 100_000, 100_000, &monster, None, None, None);

        assert_eq!(report.turns.len(), REHEARSAL_TURN_CAP);
        assert_eq!(report.end, RehearsalEnd::TurnCapReached);
    }

    /// Regra 4: cada turno reporta dano causado, ciclos usados/orçamento,
    /// truncamento e dano recebido — não só o resultado final.
    #[test]
    fn each_turn_reports_damage_cycles_and_truncation() {
        let program = parser::parse("atacar(magia.Fogo)").unwrap();
        let monster = mummy_state();
        let vars: HashMap<String, Value> = HashMap::new();

        let report = rehearse(&program, &vars, 100, 100, &monster, None, None, None);
        let first = &report.turns[0];

        assert!(first.damage_dealt > 0);
        assert_eq!(first.cycle_budget, monster.spec.cycle_budget);
        assert!(first.cycles_used > 0);
        assert!(!first.truncated);
    }
}

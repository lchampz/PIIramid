//! RFC-029: Grade de Eficiência — transforma o que o duelo já mede (ciclos
//! usados por turno, turnos gastos) numa nota S/A/B/C comparada contra o
//! próprio recorde do jogador para aquele monstro. **Nenhum cálculo novo de
//! dano/ciclo é inventado aqui** (regra 1 da RFC): a única simulação que
//! este módulo roda é a do "script de referência" de cada monstro — o
//! mesmo texto e a mesma rotina turno-a-turno que os testes
//! `*_rhythm_within_target_range` de `script/vm.rs` (RFC-022) já provam,
//! chamando `vm::run_turn` (função pública já existente) em vez de
//! reescrever a matemática de combate. Isso mantém a nota calibrada por
//! monstro (regra 2) sem hardcoded "números mágicos" que quebrariam
//! silenciosamente numa recalibração futura de `cycle_budget`/`max_life`
//! (RFC-021/022/024 já recalibraram o bestiário três vezes).
//!
//! Módulo puro: importa `monsters` e `script`, nunca `macroquad` — mesma
//! fronteira que `script/` já respeita (ver `agent/gamedev.md`).

use std::collections::HashMap;

use crate::monsters::{MonsterSpec, Posture};
use crate::script::parser;
use crate::script::vm;

/// Nota final mostrada no painel de vitória. `Ord` deriva na ordem
/// declarada (C < B < A < S) — usada só em testes para comparar notas por
/// conveniência, nunca para decidir novo recorde (isso é decidido por
/// `score`, não pela letra, ver `is_new_record`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grade {
    C,
    B,
    A,
    S,
}

impl Grade {
    pub fn label(self) -> &'static str {
        match self {
            Grade::S => "S",
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
        }
    }
}

/// Limiares de nota sobre `score` (1.0 = empatou exatamente com o script de
/// referência do monstro, ver `simulate_reference`). Calibrados para que o
/// próprio script de referência (score == 1.0 por construção, já que é
/// comparado contra si mesmo) sempre caia em `S` com folga — é o critério
/// de aceite mais importante da RFC-029, testado em
/// `reference_script_always_grades_high` abaixo.
const GRADE_S_MIN: f32 = 0.85;
const GRADE_A_MIN: f32 = 0.65;
const GRADE_B_MIN: f32 = 0.45;

fn grade_from_score(score: f32) -> Grade {
    if score >= GRADE_S_MIN {
        Grade::S
    } else if score >= GRADE_A_MIN {
        Grade::A
    } else if score >= GRADE_B_MIN {
        Grade::B
    } else {
        Grade::C
    }
}

/// O script de referência de cada monstro — texto idêntico ao que os
/// testes `*_rhythm_within_target_range` (`script/vm.rs`, RFC-022) já
/// provam como "a estratégia correta" — e se a postura do monstro alterna
/// a cada turno (fraquezas action-gated por postura: `ExigeGuarda`,
/// `DuploSelo`). Chave = `MonsterSpec.title`, o mesmo identificador estável
/// que `SaveData::best_result` usa (regra 3 da RFC).
fn reference_script(title: &str) -> Option<(String, bool)> {
    match title {
        "Mumia" => Some(("atacar(magia.Fogo)\n".repeat(3), false)),
        "Zumbi" => Some(("atacar(espada.Ferro)\n".repeat(3), false)),
        "Escaravelho" => {
            Some(("if inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(5), true))
        }
        "Esfinge" => Some(("inspecionar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(3), false)),
        "Aker" => Some((
            "inspecionar()\nif inimigo.postura == \"guarda\":\n".to_string() + &"    atacar(espada.Bronze)\n".repeat(4) + "else:\n    esperar()\n",
            true,
        )),
        "Apagado" => Some(("func golpe():\n    atacar(espada.Bronze)\n\n".to_string() + &"golpe()\n".repeat(3), false)),
        "Chabti-Mor" => {
            Some(("invocar a:\n    esperar()\ninvocar b:\n    esperar()\n".to_string() + &"atacar(espada.Bronze)\n".repeat(2), false))
        }
        _ => None,
    }
}

/// Roda o script de referência de `spec` do começo ao fim (vida cheia até
/// zerar), turno a turno, devolvendo `(turnos, ciclos_totais)`. Mesma
/// rotina de `turns_to_defeat_with_spec` em `script/vm.rs` (RFC-022) — não
/// duplicada por `#[cfg(test)]` lá porque a Grade de Eficiência precisa do
/// mesmo número em tempo de jogo, não só em teste. `None` só pode
/// acontecer se o script de referência estourar o orçamento calibrado do
/// próprio monstro (não deveria — os 7 testes de ritmo garantem isso —
/// mas esta função nunca assume, devolve `None` em vez de produzir uma
/// nota inventada sobre um número que não é o "par" de verdade).
fn simulate_reference(spec: &MonsterSpec, src: &str, toggle_posture: bool) -> Option<(u32, u32)> {
    let program = parser::parse(src).ok()?;
    let mut life = spec.max_life;
    let mut posture = Posture::Guarda;
    let mut turns = 0u32;
    let mut total_cycles = 0u32;
    while life > 0 && turns < 200 {
        let r = vm::run_turn(
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
        .ok()?;
        if r.truncated {
            return None;
        }
        life = r.enemy_life;
        total_cycles += r.cycles_used;
        if toggle_posture {
            posture = posture.toggled();
        }
        turns += 1;
    }
    if life == 0 {
        Some((turns, total_cycles))
    } else {
        None
    }
}

/// Nota final: combinação ponderada de dois fatores, cada um comparado
/// contra o script de referência **daquele monstro específico** (nunca um
/// limiar universal — regra 2 da RFC):
///
/// - `turn_factor` (peso 0.6): `turnos_referencia / turnos_reais`, capado em
///   1.0 — vencer no mesmo número de turnos do script de referência (ou
///   menos) vale crédito cheio; cada turno a mais dilui a nota. É o fator
///   dominante porque é o que o jogador *sente* diretamente (DOSSIE-003:
///   "fechei em 2 turnos, a nota SUBIU").
/// - `cycle_factor` (peso 0.4): `ciclos_medios_referencia / ciclos_medios_reais`,
///   também capado em 1.0 — usar, em média, o mesmo tanto de ciclos por
///   turno que a referência (ou menos) vale crédito cheio; um script que
///   gasta mais ciclos por turno que o necessário (loop desperdiçado,
///   `esperar()` de enchimento) perde nota mesmo vencendo no mesmo número
///   de turnos.
///
/// O script de referência comparado contra si mesmo dá `turn_factor ==
/// cycle_factor == 1.0`, `score == 1.0` — sempre `S` por construção, nunca
/// por acidente de calibração (é exatamente o critério de aceite mais
/// importante da RFC-029).
fn score_from(spec: &MonsterSpec, turns: u32, avg_cycles_per_turn: f32) -> Option<f32> {
    let (script, toggle) = reference_script(spec.title)?;
    let (turns_ref, total_cycles_ref) = simulate_reference(spec, &script, toggle)?;
    if turns == 0 || avg_cycles_per_turn <= 0.0 || turns_ref == 0 {
        return None;
    }
    let avg_ref = total_cycles_ref as f32 / turns_ref as f32;
    let turn_factor = (turns_ref as f32 / turns as f32).min(1.0);
    let cycle_factor = (avg_ref / avg_cycles_per_turn).min(1.0);
    Some(0.6 * turn_factor + 0.4 * cycle_factor)
}

/// Resultado de uma vitória, pronto para o painel mostrar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evaluation {
    pub grade: Grade,
    pub score: f32,
    pub is_new_record: bool,
}

/// RFC-029 regra 4/5: avalia o desempenho do duelo que acabou de ser
/// vencido contra `spec` e, se for melhor que o recorde salvo (ou não
/// houver recorde ainda), atualiza `best_result` — nunca regride um
/// recorde melhor (critério de aceite da RFC). `turns`/`total_cycles_used`
/// vêm só de dados que `DuelScene` já produz (`TurnResult.cycles_used`
/// acumulado turno a turno e a contagem de turnos), sem cálculo novo de
/// VM. `None` só para um `title` fora do bestiário de 7 monstros (não
/// deveria acontecer — `PHASES` é o único produtor de `MonsterSpec` em
/// jogo — mas esta função nunca inventa nota para um monstro sem script de
/// referência calibrado).
pub fn apply_duel_result(
    best_result: &mut HashMap<String, (u32, f32)>,
    spec: &MonsterSpec,
    turns: u32,
    total_cycles_used: u32,
) -> Option<Evaluation> {
    if turns == 0 {
        return None;
    }
    let avg_cycles_per_turn = total_cycles_used as f32 / turns as f32;
    let score = score_from(spec, turns, avg_cycles_per_turn)?;
    let grade = grade_from_score(score);

    let previous_score = best_result.get(spec.title).and_then(|&(pt, pc)| score_from(spec, pt, pc));
    let is_new_record = match previous_score {
        Some(prev) => score > prev,
        None => true,
    };
    if is_new_record {
        best_result.insert(spec.title.to_string(), (turns, avg_cycles_per_turn));
    }
    Some(Evaluation { grade, score, is_new_record })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monsters::PHASES;

    /// Critério de aceite mais importante da RFC-029: o script de
    /// referência calibrado de cada um dos 7 monstros produz nota alta (S
    /// ou A), não uma nota mediana por acidente de fórmula.
    #[test]
    fn reference_script_always_grades_high() {
        for (_, spec_fn) in PHASES {
            let spec = spec_fn();
            let (script, toggle) = reference_script(spec.title).unwrap_or_else(|| panic!("sem script de referencia para {}", spec.title));
            let (turns, total_cycles) = simulate_reference(&spec, &script, toggle)
                .unwrap_or_else(|| panic!("script de referencia de {} nao venceu dentro do orcamento calibrado", spec.title));

            let mut best_result = HashMap::new();
            let eval = apply_duel_result(&mut best_result, &spec, turns, total_cycles).expect("avaliacao deveria existir para monstro do bestiario");

            assert!(
                eval.grade >= Grade::A,
                "{} deveria tirar nota alta (S ou A) com o script de referencia, tirou {} (score {:.3}, turnos {}, ciclos totais {})",
                spec.title,
                eval.grade.label(),
                eval.score,
                turns,
                total_cycles
            );
            assert!(eval.is_new_record, "primeira vitoria contra {} deveria sempre gravar recorde", spec.title);
        }
    }

    #[test]
    fn worse_result_never_regresses_the_saved_record() {
        let spec = crate::monsters::data::mummy();
        let (script, toggle) = reference_script(spec.title).unwrap();
        let (turns, total_cycles) = simulate_reference(&spec, &script, toggle).unwrap();

        let mut best_result = HashMap::new();
        let first = apply_duel_result(&mut best_result, &spec, turns, total_cycles).unwrap();
        assert!(first.is_new_record);
        let stored_after_first = best_result.get(spec.title).copied();

        // Vencer de novo gastando o dobro dos turnos (bem pior) nao pode
        // sobrescrever o recorde melhor que ja estava salvo.
        let worse = apply_duel_result(&mut best_result, &spec, turns * 2, total_cycles * 2).unwrap();
        assert!(!worse.is_new_record, "resultado pior nao deveria ser sinalizado como novo recorde");
        assert_eq!(best_result.get(spec.title).copied(), stored_after_first, "recorde salvo nao pode regredir apos um resultado pior");
    }

    #[test]
    fn strictly_better_result_updates_the_record() {
        let spec = crate::monsters::data::mummy();
        let (script, toggle) = reference_script(spec.title).unwrap();
        let (turns, total_cycles) = simulate_reference(&spec, &script, toggle).unwrap();

        let mut best_result = HashMap::new();
        apply_duel_result(&mut best_result, &spec, turns * 2, total_cycles * 2).unwrap();

        // Vencer de novo com o proprio script de referencia (bem melhor)
        // precisa atualizar e ser sinalizado como novo recorde.
        let better = apply_duel_result(&mut best_result, &spec, turns, total_cycles).unwrap();
        assert!(better.is_new_record, "resultado estritamente melhor precisa ser sinalizado como novo recorde");
        assert_eq!(best_result.get(spec.title).copied(), Some((turns, total_cycles as f32 / turns as f32)));
    }

    #[test]
    fn unknown_monster_title_never_produces_an_invented_grade() {
        let mut best_result = HashMap::new();
        let fake = MonsterSpec {
            title: "Nao Existe No Bestiario",
            room: "",
            description: ["", ""],
            max_life: 10,
            cycle_budget: 10,
            weakness: crate::monsters::Weakness::Elemento(crate::monsters::Element::Nenhum),
            base_damage: 1,
            attack_name: "",
            special_attack_name: "",
            drop: crate::inventory::Item { id: "x".into(), kind: crate::script::value::ItemKind::Espada, name: "x".into(), bonus_damage: 0 },
        };
        assert_eq!(apply_duel_result(&mut best_result, &fake, 3, 9), None);
    }

    #[test]
    fn grade_thresholds_are_ordered_and_exhaustive() {
        assert_eq!(grade_from_score(1.0), Grade::S);
        assert_eq!(grade_from_score(GRADE_S_MIN), Grade::S);
        assert_eq!(grade_from_score(GRADE_S_MIN - 0.01), Grade::A);
        assert_eq!(grade_from_score(GRADE_A_MIN), Grade::A);
        assert_eq!(grade_from_score(GRADE_A_MIN - 0.01), Grade::B);
        assert_eq!(grade_from_score(GRADE_B_MIN), Grade::B);
        assert_eq!(grade_from_score(GRADE_B_MIN - 0.01), Grade::C);
        assert_eq!(grade_from_score(0.0), Grade::C);
    }
}

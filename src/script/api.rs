//! A API nativa que o pseudo-código pode chamar, e o custo em ciclos de
//! cada chamada — é essa tabela que faz "melhor algoritmo ganha" ser
//! literal: cada `atacar()`/`defender()`/etc. debita do orçamento do
//! turno, e um `while`/`for` que repete a chamada demais estoura antes de
//! terminar.

/// Custo em ciclos de cada função nativa. `None` significa que o nome não
/// é uma função conhecida (erro de execução).
pub fn call_cost(name: &str) -> Option<u32> {
    match name {
        "atacar" => Some(2),
        "defender" => Some(1),
        "inspecionar" => Some(3),
        "curar" => Some(4),
        "esperar" => Some(1),
        _ => None,
    }
}

/// Custo fixo de avaliar a condição de um `while`/`for` a cada iteração
/// (incluindo a avaliação final que encerra o laço) — é o que faz um
/// algoritmo O(n²) custar muito mais que um O(n) mesmo com o mesmo corpo.
pub const LOOP_TICK_COST: u32 = 1;
/// Custo de avaliar a condição de um `if`.
pub const BRANCH_COST: u32 = 1;

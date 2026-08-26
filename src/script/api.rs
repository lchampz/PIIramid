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

/// Custo em ciclos de invocar uma função definida pelo jogador (RFC-006,
/// regra 9). Cobrado *antes* de executar o corpo, somado ao custo das
/// instruções do corpo — `combo()` chamado 3 vezes custa 3 vezes o corpo
/// mais 3 ciclos de invocação, nunca de graça. É também o que faz recursão
/// infinita truncar pelo orçamento em vez de derrubar o jogo: sem esse
/// custo, `func f(): f()` nunca gastaria ciclo e recorreria até estourar a
/// pilha do Rust — um abort de processo que nenhum `Result` captura.
pub const USER_CALL_COST: u32 = 1;

/// Rede de segurança de engenharia, independente do orçamento de ciclos:
/// nenhum monstro do bestiário tem orçamento ≥ 32 (o maior é a Esfinge,
/// 24), então este limite nunca deveria disparar antes do orçamento —
/// se disparar, é sinal de que algum caminho futuro zerou o custo por
/// invocação e a rede de segurança pegou o que o design deveria ter pego.
pub const MAX_CALL_DEPTH: usize = 32;

/// Bônus de dano por classe (RFC-003 §1), somado em `resolve_attack`
/// (`script/vm.rs`) depois do bônus de item equipado, quando
/// `player_class.affinity() == item.kind`. Aditivo fixo, nunca
/// multiplicativo (decisão de engenharia da RFC-003: um multiplicador
/// aplicado depois da fraqueza inflaria também os danos já reduzidos por
/// antijogo, mudando a margem entre estratégias travada pelas RFC-008/
/// 011/012). `4`: mesma ordem de grandeza dos `bonus_damage` de item de
/// exemplo em `inventory.rs` (3, 6, 8 no save de teste) — perceptível, mas
/// menor que a diferença entre dano efetivo e reduzido por fraqueza
/// (`BASE_ATTACK_DAMAGE` 12 contra `BASE_ATTACK_DAMAGE / 3` = 4, `/ 4` =
/// 3), então a classe nunca disfarça a decisão de fraqueza errada.
pub const CLASS_BONUS_DAMAGE: i32 = 4;

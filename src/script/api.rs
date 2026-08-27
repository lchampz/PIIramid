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

/// Custo em ciclos de **ter escrito** um `Stmt`, cobrado uma única vez por
/// instrução da árvore (RFC-024) — não por quantas vezes ela executa. É o
/// que desfaz a inversão medida em `ANALISE-por-que-o-jogo-e-facil.md`
/// (causa 3): antes desta RFC, `LOOP_TICK_COST`/`BRANCH_COST` só cobravam
/// *execução*, então desenrolar N ataques (`2N` ciclos) sempre batia
/// qualquer laço — copiar-colar era estritamente melhor.
///
/// Com `STMT_SIZE_COST` somado, compare `atacar()` copiado N vezes contra
/// `for i in 0..N: atacar(...)` (2 `Stmt` escritos, corpo fixo):
///
/// - desenrolado: `N` `Stmt` escritos, `2` ciclos de execução cada →
///   `(STMT_SIZE_COST + 2) * N`
/// - laço: `2` `Stmt` escritos (fixo, não cresce com N) + `LOOP_TICK_COST`
///   por iteração (o `for` cobra só nas N passagens reais — a checagem
///   final que sai do laço é um teste nativo do host, não um `Stmt`
///   cobrado; `while` cobra também a checagem de saída, por isso custa
///   1 ciclo a mais que um `for` equivalente) + `2` de `atacar()` por
///   iteração → `STMT_SIZE_COST * 2 + (LOOP_TICK_COST + 2) * N`
///
/// Com `STMT_SIZE_COST = 2` e `LOOP_TICK_COST = 1`: desenrolado `4N`,
/// laço `4 + 3N` — desenrolado vence até `N=3` (12 < 13), empata em
/// `N=4` (16 = 16), laço vence a partir de `N=5` (19 < 20). Bate com a
/// tabela modelada pelo product-manager na RFC-024. Ver o teste de ponto
/// de virada (`unrolled_wins_small_n_loop_wins_large_n_same_damage`,
/// `script/vm.rs`) pela prova via VM real, não só a conta em texto.
/// Unidade é `Stmt`, nunca caractere/token (não-objetivo 3 da RFC) —
/// comentário e linha vazia não são `Stmt`, não custam nada.
pub const STMT_SIZE_COST: u32 = 2;

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

/// Custo em ciclos, cobrado do orçamento **principal** do turno, de
/// encontrar um `invocar nome:` (RFC-004, regra 3) — antes mesmo de rodar
/// o corpo. Estourar o orçamento principal aqui se comporta exatamente
/// como estourar em qualquer outra instrução: `Signal::Truncated` normal,
/// contra-ataque incluído. Não é o custo do corpo da invocação — esse é
/// pago do pool separado `INVOKE_BUDGET`.
pub const INVOKE_COST: u32 = 2;

/// Sub-orçamento de ciclos próprio e fixo para o corpo de um `invocar`
/// (RFC-004, regra 4). Calibrado para caber exatamente um `atacar()` (2
/// ciclos) com folga, ou dois com zero folga — pequeno de propósito: é
/// reforço, não substituto do script principal. Ciclo sobrando aqui nunca
/// gera `BonusStrike` nem volta ao orçamento principal (não-objetivo 5 da
/// RFC) — é gasto ou perdido, nunca acumulado.
pub const INVOKE_BUDGET: u32 = 4;

/// Limite de invocações por turno (RFC-004, regra 5), verificado em
/// runtime pelo campo `Vm::invocations_this_turn` — mesmo padrão de
/// `MAX_CALL_DEPTH`. `2` bate com o exemplo exato da issue original
/// (esqueleto + mago morto).
pub const MAX_INVOCATIONS_PER_TURN: usize = 2;

/// Custo em ciclos de examinar **um** item da mochila dentro de
/// `selecionar()` (RFC-015, regra 7). Cobrado por item examinado, não por
/// tamanho da mochila (regra 8) — é o que faz reordenar cláusulas `and`
/// dentro de `onde:` mudar o custo real: o curto-circuito que
/// `eval_binary` já implementa decide quantas cláusulas avaliar por item,
/// mas o número de *itens* examinados até o primeiro match depende de onde
/// na mochila ele está.
pub const SELECT_SCAN_COST: u32 = 1;

//! Catálogo de monstros. As descrições reaproveitam o texto original do
//! jogo em C (`consts.h`), só que agora explicam a fraqueza *algorítmica*
//! em vez da fraqueza aritmética.

use super::{Element, MonsterSpec, Weakness};

/// RFC-022: primeiro monstro da progressao (RFC-005), turno-alvo 3. O
/// orcamento de 20 originais sobrava demais -- 10x `atacar()` cabiam no
/// mesmo turno (2 ciclos cada) e o combate se resolvia num unico turno com
/// o elemento certo, longe da faixa `3..=6`. `cycle_budget` cai para 6 (so
/// 3 ataques por turno); `max_life` continua 100. Calibrado e provado por
/// `mummy_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn mummy() -> MonsterSpec {
    MonsterSpec {
        title: "Mumia",
        room: "Corredor das Mariposas",
        description: [
            "Provavelmente algum farao carcumido por vermes...",
            "apesar das mariposas acharem suas ataduras apetitosas, seu maior medo e o fogo.",
        ],
        max_life: 100,
        cycle_budget: 6,
        weakness: Weakness::Elemento(Element::Fogo),
        base_damage: 8,
        attack_name: "Atadura Viva",
        special_attack_name: "Maldicao do Escaravelho",
    }
}

/// RFC-021, achado adicional: com `cycle_budget == max_ciclos` (8 == 8, como
/// era antes desta RFC) e estruturalmente impossivel um turno legal (que nao
/// estoure o proprio orcamento) jamais ultrapassar `max_ciclos` -- a
/// fraqueza nunca acertava a reducao de verdade em jogo, so no papel. O
/// teste de ordenacao `zombie_naive_waste_never_beats_efficient_script`
/// (`src/script/vm.rs`) exigiu abrir essa folga: `cycle_budget` sobe pra 16
/// (o dobro), mas `max_ciclos` continua 8 -- a *condicao* da fraqueza nao
/// muda (RFC-021 nao-objetivo 3), so o orcamento do monstro, que agora
/// sobra espaco pra um script perdulario (ex.: `esperar()` de enchimento)
/// realmente furar o limite de eficiencia e pagar o preco.
/// RFC-022: segundo monstro, turno-alvo 3. `cycle_budget` continua 16 --
/// baixar isso reabriria o bug que a RFC-021 corrigiu (com orcamento <=
/// `max_ciclos`=8, nenhum turno legal ultrapassa o limite, e a fraqueza
/// nunca observa o script perdulario de verdade). Só `max_life` sobe
/// (80 -> 120) para que a estrategia correta (3x `atacar()`, 6 ciclos,
/// bem abaixo de `max_ciclos`) leve 3 turnos em vez de 2. Calibrado e
/// provado por `zombie_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn zombie() -> MonsterSpec {
    MonsterSpec {
        title: "Zumbi",
        room: "Fossa dos Sussurros",
        description: [
            "Ceeeeeeeerebros... Ceeeeeeeeeeerebros...",
            "Um defunto sedento por cerebros, lento e burro: so aguenta scripts curtos.",
        ],
        max_life: 120,
        cycle_budget: 16,
        weakness: Weakness::Eficiencia { max_ciclos: 8 },
        base_damage: 6,
        attack_name: "Mordida Podre",
        special_attack_name: "Enxame Cadaverico",
    }
}

/// RFC-021: `ExigeGuarda` e a unica fraqueza cuja condicao e *ambiente*
/// (postura alterna sozinha, `Posture::toggled`) em vez de exigir uma acao
/// do jogador -- um script cego (sem `if`) ja acerta dano cheio em ~metade
/// dos turnos, de graca. Isso limita o quanto qualquer divisor pune: com um
/// `cycle_budget` par, o unico ciclo do `if` de bifurcacao custava um
/// ataque inteiro no turno de guarda, tornando o spam cego competitivo ou
/// ate melhor que o script correto (antijogo). O orcamento precisa ficar
/// impar pra o `if` caber sem custar um ataque -- ver
/// `beetle_naive_spam_never_beats_posture_branch` (`src/script/vm.rs`).
///
/// RFC-022, turno-alvo 4: a fraqueza tem margem estruturalmente mais fina
/// que as outras (RFC-021, mesma nota acima) -- por isso o `cycle_budget`
/// so foi reduzido com cautela (17 -> 11, ainda impar, ainda cabendo 5
/// ataques inteiros no turno de guarda) em vez de raspado ao minimo, o que
/// reabriria o antijogo (testado manualmente antes de fechar o numero:
/// com orcamento 7/vida 80 o spam cego empatava em turnos com o script
/// correto). `max_life` sobe de 110 para 135 pra fechar o turno-alvo com
/// esse orcamento. Calibrado e provado por
/// `beetle_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn beetle() -> MonsterSpec {
    MonsterSpec {
        title: "Escaravelho",
        room: "Camara dos Escaravelhos",
        description: [
            "Uma carapaca dura que alterna entre postura de guarda e aberta.",
            "So toma dano de verdade se voce ler a postura e reagir a ela.",
        ],
        max_life: 135,
        cycle_budget: 11,
        weakness: Weakness::ExigeGuarda,
        base_damage: 7,
        attack_name: "Investida da Carapaca",
        special_attack_name: "Turbilhao de Areia",
    }
}

/// RFC-022: quarto monstro, turno-alvo 4. `RequerInspecao` bloqueia dano
/// por completo sem `inspecionar()` (nao so reduz) -- entre as 7 fraquezas
/// e a que tem menos risco de reabrir antijogo com qualquer recalibracao,
/// entao so o `cycle_budget` mudou (24 -> 9, exatamente
/// `inspecionar()`(3) + 3x `atacar()`(2 cada) = 9, sem sobra); `max_life`
/// continua 140. Calibrado e provado por `sphinx_rhythm_within_target_range`
/// (`src/script/vm.rs`).
pub fn sphinx() -> MonsterSpec {
    MonsterSpec {
        title: "Esfinge",
        room: "Antecamara Selada",
        description: [
            "Guarda um segredo: nao revela a fraqueza para quem nao pergunta.",
            "Inspecione antes de atacar, ou nenhum golpe vai valer nada.",
        ],
        max_life: 140,
        cycle_budget: 9,
        weakness: Weakness::RequerInspecao,
        base_damage: 10,
        attack_name: "Enigma Cortante",
        special_attack_name: "Julgamento da Esfinge",
    }
}

/// RFC-008: quinto monstro, o "boss cumulativo" depois dos quatro. A
/// fraqueza (`Weakness::DuploSelo`) nao ensina conceito novo -- exige que
/// as duas licoes ja dadas separadamente por `beetle()` (postura) e
/// `sphinx()` (inspecao) valham ao mesmo tempo.
///
/// RFC-022, turno-alvo 5: o `cycle_budget` de 10 era a causa raiz do
/// "atoleiro" de 15 turnos medido pelo product-manager -- so cabia UM
/// `atacar()` depois de pagar `inspecionar()`(3) + `if`(1), e o resto do
/// orcamento virava golpe-bonus (1 dano/ciclo), muito pior por ciclo que
/// um ataque de verdade (12 dano por 2 ciclos). A correcao e por regra da
/// RFC: sobe o orcamento, nao corta a vida. `cycle_budget` 10 -> 12 cabe
/// uma sequencia real de 4 ataques no turno de guarda (3+1+4*2=12, exato);
/// `max_life` continua 150, sem alteracao -- o orcamento maior sozinho ja
/// fecha o turno-alvo. Calibrado e provado por
/// `guardiao_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn guardiao() -> MonsterSpec {
    MonsterSpec {
        title: "Aker",
        room: "Camara do Duplo Limiar",
        description: [
            "Guarda duas portas gemeas do horizonte: ontem e amanha.",
            "So abre as duas ao mesmo tempo - nunca uma antes da outra.",
        ],
        max_life: 150,
        cycle_budget: 12,
        weakness: Weakness::DuploSelo,
        base_damage: 9,
        attack_name: "Mordida do Horizonte",
        special_attack_name: "Selo dos Dois Sois",
    }
}

/// RFC-012: sexto monstro, primeira fraqueza que julga a *forma* do script
/// (de onde saiu o `atacar()`), nao o estado do combate. A estrategia
/// correta (`func` com um `atacar()` dentro, chamada repetidamente) fecha o
/// combate em bem menos turnos que o spam ingenuo de `atacar()` solto no
/// corpo principal -- disciplina de teste de ordenacao provada por
/// `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns`
/// (`src/script/vm.rs`), que usa budget/vida fixos proprios (16/150,
/// decoupled do bestiario) e continua intocada.
///
/// RFC-022, turno-alvo 5: com o bestiario real (`max_life`/`cycle_budget`
/// abaixo), 3x `golpe()` (USER_CALL_COST(1)+atacar(2)=3 ciclos cada, 9 no
/// total, orcamento 9 exato) fecha em 5 turnos com vida 160 -- orcamento
/// caiu de 16 para 9 e vida subiu de 150 para 160. Calibrado e provado por
/// `sentinela_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn sentinela() -> MonsterSpec {
    MonsterSpec {
        title: "Apagado",
        room: "Camara das Palavras Verdadeiras",
        description: [
            "Um escriba cujo proprio nome a piramide riscou da pedra.",
            "Para ele, o que nao tem nome nunca aconteceu de verdade.",
        ],
        max_life: 160,
        cycle_budget: 9,
        weakness: Weakness::ExigeNomeacao,
        base_damage: 8,
        attack_name: "Traco Riscado",
        special_attack_name: "Veredito do Nome Verdadeiro",
    }
}

/// RFC-017: setimo monstro, fecha o ciclo de `invocar` (RFC-004) --
/// primeira fraqueza que exige a mecanica de invocacao em vez de so
/// permiti-la. A estrategia correta (2x `invocar` pagando `2*INVOKE_COST`,
/// depois `atacar()` com dano cheio) fecha o combate em bem menos turnos
/// que o spam ingenuo de `atacar()` solto, sem invocar -- disciplina de
/// teste de ordenacao provada por
/// `exige_invocacao_dupla_beats_naive_spam_in_fewer_turns`
/// (`src/script/vm.rs`), que usa budget/vida fixos proprios (12/150,
/// decoupled do bestiario) e continua intocada.
///
/// RFC-022, turno-alvo 6 -- o mais longo da progressao, ultimo monstro:
/// 2x `invocar` (2*INVOKE_COST=4) + 2x `atacar()` (4) = 8 ciclos, orcamento
/// 8 exato, dano cheio fecha em 6 turnos com vida 140. Orcamento caiu de 12
/// para 8 e vida caiu de 150 para 140. Calibrado e provado por
/// `necroguardiao_rhythm_within_target_range` (`src/script/vm.rs`).
pub fn necroguardiao() -> MonsterSpec {
    MonsterSpec {
        title: "Chabti-Mor",
        room: "Cripta dos Chabtis Sem Nome",
        description: [
            "Em vida, nunca ergueu a propria mao -- so dava ordens aos chabtis.",
            "So golpeia de verdade depois de chamar reforcos duas vezes.",
        ],
        max_life: 140,
        cycle_budget: 8,
        weakness: Weakness::ExigeInvocacaoDupla,
        base_damage: 9,
        attack_name: "Punho Sem Pratica",
        special_attack_name: "Chamado dos Tres Chabtis",
    }
}

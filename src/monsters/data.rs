//! Catálogo de monstros. As descrições reaproveitam o texto original do
//! jogo em C (`consts.h`), só que agora explicam a fraqueza *algorítmica*
//! em vez da fraqueza aritmética.

use super::{Element, MonsterSpec, Weakness};

pub fn mummy() -> MonsterSpec {
    MonsterSpec {
        title: "Mumia",
        room: "Corredor das Mariposas",
        description: [
            "Provavelmente algum farao carcumido por vermes...",
            "apesar das mariposas acharem suas ataduras apetitosas, seu maior medo e o fogo.",
        ],
        max_life: 100,
        cycle_budget: 20,
        weakness: Weakness::Elemento(Element::Fogo),
        base_damage: 8,
        attack_name: "Atadura Viva",
        special_attack_name: "Maldicao do Escaravelho",
    }
}

pub fn zombie() -> MonsterSpec {
    MonsterSpec {
        title: "Zumbi",
        room: "Fossa dos Sussurros",
        description: [
            "Ceeeeeeeerebros... Ceeeeeeeeeeerebros...",
            "Um defunto sedento por cerebros, lento e burro: so aguenta scripts curtos.",
        ],
        max_life: 80,
        cycle_budget: 8,
        weakness: Weakness::Eficiencia { max_ciclos: 8 },
        base_damage: 6,
        attack_name: "Mordida Podre",
        special_attack_name: "Enxame Cadaverico",
    }
}

pub fn beetle() -> MonsterSpec {
    MonsterSpec {
        title: "Escaravelho",
        room: "Camara dos Escaravelhos",
        description: [
            "Uma carapaca dura que alterna entre postura de guarda e aberta.",
            "So toma dano de verdade se voce ler a postura e reagir a ela.",
        ],
        max_life: 90,
        cycle_budget: 16,
        weakness: Weakness::ExigeGuarda,
        base_damage: 7,
        attack_name: "Investida da Carapaca",
        special_attack_name: "Turbilhao de Areia",
    }
}

pub fn sphinx() -> MonsterSpec {
    MonsterSpec {
        title: "Esfinge",
        room: "Antecamara Selada",
        description: [
            "Guarda um segredo: nao revela a fraqueza para quem nao pergunta.",
            "Inspecione antes de atacar, ou nenhum golpe vai valer nada.",
        ],
        max_life: 140,
        cycle_budget: 24,
        weakness: Weakness::RequerInspecao,
        base_damage: 10,
        attack_name: "Enigma Cortante",
        special_attack_name: "Julgamento da Esfinge",
    }
}

/// RFC-008: quinto monstro, o "boss cumulativo" depois dos quatro. A
/// fraqueza (`Weakness::DuploSelo`) nao ensina conceito novo -- exige que
/// as duas licoes ja dadas separadamente por `beetle()` (postura) e
/// `sphinx()` (inspecao) valham ao mesmo tempo. Vida e orcamento mais
/// altos que os 4 atuais (80-140 / 8-24), calibrados e provados pelo teste
/// `duplo_selo_reference_script_wins_within_calibrated_budget`
/// (`src/script/vm.rs`): 10 ciclos cabem o script de referencia
/// (inspecionar + if + atacar = 6 no pior caso) sem sobra excessiva, e sem
/// apertar tanto que a composicao pareca impossivel.
pub fn guardiao() -> MonsterSpec {
    MonsterSpec {
        title: "Aker",
        room: "Camara do Duplo Limiar",
        description: [
            "Guarda duas portas gemeas do horizonte: ontem e amanha.",
            "So abre as duas ao mesmo tempo - nunca uma antes da outra.",
        ],
        max_life: 150,
        cycle_budget: 10,
        weakness: Weakness::DuploSelo,
        base_damage: 9,
        attack_name: "Mordida do Horizonte",
        special_attack_name: "Selo dos Dois Sois",
    }
}

/// RFC-012: sexto monstro, primeira fraqueza que julga a *forma* do script
/// (de onde saiu o `atacar()`), nao o estado do combate. Vida 150 e
/// orcamento 16 calibrados pelo teste
/// `exige_nomeacao_named_func_beats_naive_spam_in_fewer_turns`
/// (`src/script/vm.rs`): com o divisor `/4` de `resolve_attack`, a
/// estrategia correta (`func` com um `atacar()` dentro, chamada
/// repetidamente) fecha o combate em bem menos turnos que o spam ingenuo de
/// `atacar()` solto no corpo principal -- a mesma disciplina de teste de
/// ordenação que a RFC-011 exigiu depois do fato, aqui desde o inicio.
pub fn sentinela() -> MonsterSpec {
    MonsterSpec {
        title: "Apagado",
        room: "Camara das Palavras Verdadeiras",
        description: [
            "Um escriba cujo proprio nome a piramide riscou da pedra.",
            "Para ele, o que nao tem nome nunca aconteceu de verdade.",
        ],
        max_life: 150,
        cycle_budget: 16,
        weakness: Weakness::ExigeNomeacao,
        base_damage: 8,
        attack_name: "Traco Riscado",
        special_attack_name: "Veredito do Nome Verdadeiro",
    }
}

/// RFC-017: setimo monstro, fecha o ciclo de `invocar` (RFC-004) --
/// primeira fraqueza que exige a mecanica de invocacao em vez de so
/// permiti-la. Vida 150 e orcamento 12 calibrados pelo teste
/// `exige_invocacao_dupla_beats_naive_spam_in_fewer_turns`
/// (`src/script/vm.rs`): com o divisor `/4` de `resolve_attack`, a
/// estrategia correta (2x `invocar` pagando `2*INVOKE_COST`, depois
/// `atacar()` 4x com dano cheio) fecha o combate em bem menos turnos que o
/// spam ingenuo de `atacar()` solto, sem invocar -- mesma disciplina de
/// teste de ordenacao desde a RFC-011/012.
pub fn necroguardiao() -> MonsterSpec {
    MonsterSpec {
        title: "Chabti-Mor",
        room: "Cripta dos Chabtis Sem Nome",
        description: [
            "Em vida, nunca ergueu a propria mao -- so dava ordens aos chabtis.",
            "So golpeia de verdade depois de chamar reforcos duas vezes.",
        ],
        max_life: 150,
        cycle_budget: 12,
        weakness: Weakness::ExigeInvocacaoDupla,
        base_damage: 9,
        attack_name: "Punho Sem Pratica",
        special_attack_name: "Chamado dos Tres Chabtis",
    }
}

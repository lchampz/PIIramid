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

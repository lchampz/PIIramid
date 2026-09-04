//! Monstros do duelo: postura, elemento e fraqueza. É a fraqueza que
//! obriga o jogador a *programar* em vez de decorar uma resposta fixa —
//! a postura muda a cada turno, então um script sem `if` não vence.

pub mod data;

use crate::inventory::Item as DropItem;
use crate::world::entity::Kind;

/// RFC-005 regra 1: registro central e único da ordem dos 7 monstros/fases
/// — nem `OverworldScene` (que ainda os espalha manualmente pelo mapa,
/// intocado) nem `PhaseScene` (que só lê daqui) duplicam essa ordem. É a
/// mesma ordem de introdução já implícita no bestiário (Múmia, Zumbi,
/// Escaravelho, Esfinge, Aker, Apagado, Chabti-Mor) — RFC-005 não-objetivo 3
/// proíbe reordenar por dificuldade aqui.
pub const PHASES: [(Kind, fn() -> MonsterSpec); 7] = [
    (Kind::Mummy, data::mummy),
    (Kind::Zombie, data::zombie),
    (Kind::Beetle, data::beetle),
    (Kind::Sphinx, data::sphinx),
    (Kind::Guardiao, data::guardiao),
    (Kind::Sentinela, data::sentinela),
    (Kind::Necroguardiao, data::necroguardiao),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Fogo,
    Agua,
    Terra,
    Nenhum,
}

impl Element {
    pub fn from_name(name: &str) -> Element {
        match name {
            "fogo" => Element::Fogo,
            "agua" => Element::Agua,
            "terra" => Element::Terra,
            _ => Element::Nenhum,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    Guarda,
    Aberta,
}

impl Posture {
    pub fn as_str(self) -> &'static str {
        match self {
            Posture::Guarda => "guarda",
            Posture::Aberta => "aberta",
        }
    }

    pub fn toggled(self) -> Posture {
        match self {
            Posture::Guarda => Posture::Aberta,
            Posture::Aberta => Posture::Guarda,
        }
    }
}

/// O que faz um monstro tomar dano de verdade — o "algoritmo certo" para
/// cada um é diferente.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Weakness {
    /// só toma dano cheio de um elemento específico; outros ataques causam
    /// dano reduzido
    Elemento(Element),
    /// só toma dano se `defender()` for chamado no turno em que ele está
    /// na postura "guarda" — ataques puros são parcialmente bloqueados
    ExigeGuarda,
    /// imune a scripts que gastem mais ciclos que `max_ciclos`: recompensa
    /// algoritmo enxuto
    Eficiencia { max_ciclos: u32 },
    /// imune a dano até `inspecionar()` ser chamado neste turno
    RequerInspecao,
    /// composição das duas fraquezas de estado já existentes: só toma dano
    /// cheio quando a postura está em "guarda" **e** `inspecionar()` já foi
    /// chamado neste turno — nenhuma condição isolada basta. É o "boss
    /// cumulativo": testa se o jogador sabe compor duas lições separadas
    /// (`ExigeGuarda`, `RequerInspecao`) com `and`, em vez de escolher uma.
    DuploSelo,
    /// RFC-012: primeira fraqueza que julga a *forma* do script, não o
    /// estado do combate. Só toma dano cheio quando `atacar()` roda de
    /// dentro de uma `func` nomeada pelo jogador (`Vm::depth > 0` no
    /// momento do golpe, RFC-006); o mesmo ataque solto no corpo principal
    /// do script (`depth == 0`) causa dano reduzido. Nomear o golpe é a
    /// lição, não estruturar em profundidade — qualquer `func` com um
    /// `atacar()` dentro já resolve.
    ExigeNomeacao,
    /// RFC-017: fecha o ciclo de `invocar` (RFC-004) dando a ele um
    /// monstro dedicado. Só toma dano cheio quando `self.invocations_this_turn`
    /// (RFC-004, campo já existente, lido sem novo estado) já chegou a 2 no
    /// turno — em qualquer ordem, dentro ou fora das invocações (RFC-017
    /// não-objetivo 4). Ensina que pagar `2×INVOKE_COST` para destravar dano
    /// cheio é uma decisão estratégica que compensa.
    ExigeInvocacaoDupla,
}

impl Weakness {
    /// rótulo curto pro card do monstro na tela de duelo
    pub fn label(self) -> &'static str {
        match self {
            Weakness::Elemento(Element::Fogo) => "FRAQUEZA FOGO",
            Weakness::Elemento(Element::Agua) => "FRAQUEZA AGUA",
            Weakness::Elemento(Element::Terra) => "FRAQUEZA TERRA",
            Weakness::Elemento(Element::Nenhum) => "SEM ELEMENTO",
            Weakness::ExigeGuarda => "EXPOSTO NA GUARDA",
            Weakness::Eficiencia { .. } => "PUNE CICLOS ALTOS",
            Weakness::RequerInspecao => "OCULTA A FRAQUEZA",
            Weakness::DuploSelo => "EXIGE GUARDA E INSPECAO",
            Weakness::ExigeNomeacao => "SO RESPEITA GOLPE NOMEADO",
            Weakness::ExigeInvocacaoDupla => "EXIGE DUAS INVOCACOES",
        }
    }
}

/// quanto a carga do inimigo sobe a cada turno, e o limiar em que o
/// contra-ataque de fim de turno vira um golpe especial (maior dano).
pub const CHARGE_PER_TURN: u32 = 7;
pub const CHARGE_THRESHOLD: u32 = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct MonsterSpec {
    pub title: &'static str,
    /// nome da câmara onde esse monstro é encontrado, para o cabeçalho do duelo
    pub room: &'static str,
    pub description: [&'static str; 2],
    pub max_life: i32,
    pub cycle_budget: u32,
    pub weakness: Weakness,
    pub base_damage: i32,
    /// nome de sabor do ataque normal e do golpe especial (carga cheia)
    pub attack_name: &'static str,
    pub special_attack_name: &'static str,
    /// RFC-028: item real (RFC-002) que este monstro deixa cair ao ser
    /// vencido — sempre o mesmo item, sem sorteio (regra 1/não-objetivo 1
    /// da RFC). Inserido em `SaveData::bag` só no braço `Won` de
    /// `PhaseScene::update` (`scenes/phase.rs`), nunca em derrota/fuga.
    pub drop: DropItem,
}

/// RFC-027: `Clone`/`PartialEq` existem só para o Ensaio Geral — clonar um
/// `MonsterState` inteiro (spec + vida + postura + carga) é o que permite
/// simular turnos futuros sobre uma cópia sem tocar o original, e comparar
/// bit-a-bit antes/depois de ensaiar é o teste que prova que nada vazou.
#[derive(Debug, Clone, PartialEq)]
pub struct MonsterState {
    pub spec: MonsterSpec,
    pub life: i32,
    pub posture: Posture,
    pub inspected_this_turn: bool,
    /// sobe a cada turno; ao atingir `CHARGE_THRESHOLD`, o próximo
    /// contra-ataque (se acontecer) vira um golpe especial e a carga zera
    pub charge: u32,
}

impl MonsterState {
    pub fn new(spec: MonsterSpec) -> Self {
        let life = spec.max_life;
        MonsterState { spec, life, posture: Posture::Guarda, inspected_this_turn: false, charge: 0 }
    }

    pub fn alive(&self) -> bool {
        self.life > 0
    }

    pub fn begin_turn(&mut self) {
        self.posture = self.posture.toggled();
        self.inspected_this_turn = false;
        self.charge += CHARGE_PER_TURN;
    }

    /// verdadeiro quando o contra-ataque desta rodada, se acontecer, é o
    /// golpe especial — usado tanto pela VM (calcula o dano) quanto pela
    /// UI (telegrafa a intenção do monstro antes do jogador agir)
    pub fn special_ready(&self) -> bool {
        self.charge >= CHARGE_THRESHOLD
    }

    /// zera a carga depois que o golpe especial realmente aconteceu
    pub fn consume_charge(&mut self) {
        self.charge = 0;
    }
}

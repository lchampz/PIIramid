//! Inventário/equipamento real do jogador (RFC-002). Antes desta RFC, o
//! Grimório era inteiramente mockado (`scenes/grimoire.rs`) e todo item
//! citado num script (`espada["ferro"]`) era só sintaxe — nunca "existia"
//! de verdade. Este módulo é a primeira fonte de estado persistente do
//! jogo (serializado em JSON ao lado do executável) e a ponte entre esse
//! estado e a VM: `Loadout` é o que `resolve_attack` (`script/vm.rs`) lê
//! pra somar bônus de dano.
//!
//! **Fronteira deliberada:** `serde`/`serde_json` só entram aqui. `ItemKind`
//! é definido em `script::value` (lógica pura, livre de dependência além de
//! `std`) e não recebe `#[derive(Serialize, Deserialize)]` — em vez disso,
//! implementamos os traits manualmente neste módulo, serializando pelo
//! nome estável que a linguagem já usa (`ItemKind::label()`/`from_ident()`).
//! Isso preserva a regra da RFC-001/002: `src/script/` nunca importa nada
//! além de `std`, mesmo que o *tipo* `ItemKind` seja usado aqui fora.
//!
//! **Item ausente nunca é erro** (decisão de engenharia mais importante da
//! RFC-002): tudo aqui é aditivo. Ausência de save, save corrompido, ou
//! ausência de item equipado sempre degradam pro comportamento anterior à
//! RFC-002 — nunca pro pânico.

use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::script::value::ItemKind;

/// Serializa `ItemKind` pelo nome usado no pseudo-código (`label()`), não
/// pelo nome da variante Rust — assim o JSON de save fica legível
/// (`"kind": "espada"`) e continua estável se a ordem das variantes mudar.
impl Serialize for ItemKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

impl<'de> Deserialize<'de> for ItemKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        ItemKind::from_ident(&s).ok_or_else(|| serde::de::Error::custom(format!("ItemKind desconhecido no save: '{s}'")))
    }
}

/// Um item real de inventário. Diferente de `script::value::Item` (que é
/// só `{kind, name}`, construído puramente da sintaxe do script e sem
/// checar se o jogador "tem" aquilo), este `Item` é o dado persistente:
/// tem identidade estável (`id`) e o bônus que ele concede quando
/// equipado e citado corretamente num script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// slug estável, ex.: "khopesh_trincado" — usado como chave de
    /// identidade (não muda se o nome de exibição mudar).
    pub id: String,
    pub kind: ItemKind,
    /// nome usado no pseudo-código, ex.: "ferro" — comparado
    /// case-insensitive contra `script::value::Item::name` em
    /// `resolve_attack`.
    pub name: String,
    /// aplicado só quando o item resolve com sucesso no slot certo
    /// (RFC-002, regra 6).
    pub bonus_damage: i32,
}

/// Os quatro slots reais (RFC-002, não-objetivo 3: Amuleto/Relíquia
/// continuam mockados — `ItemKind` só tem estas 4 variantes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub arma: Option<Item>,
    pub magia: Option<Item>,
    pub escudo: Option<Item>,
    pub pocao: Option<Item>,
}

impl Loadout {
    /// Item equipado no slot correspondente ao `kind`, se houver — é o
    /// que `resolve_attack` consulta pra decidir o bônus (RFC-002, regra
    /// 6). `match` exaustivo: uma variante nova em `ItemKind` (não deveria
    /// acontecer sem RFC própria, ver não-objetivo 3) quebra a compilação
    /// aqui em vez de cair num slot errado em silêncio.
    pub fn slot(&self, kind: ItemKind) -> Option<&Item> {
        match kind {
            ItemKind::Espada => self.arma.as_ref(),
            ItemKind::Magia => self.magia.as_ref(),
            ItemKind::Escudo => self.escudo.as_ref(),
            ItemKind::Pocao => self.pocao.as_ref(),
        }
    }

    /// Slot mutável correspondente — usado por "equipar"/"desequipar" no
    /// Grimório.
    pub fn slot_mut(&mut self, kind: ItemKind) -> &mut Option<Item> {
        match kind {
            ItemKind::Espada => &mut self.arma,
            ItemKind::Magia => &mut self.magia,
            ItemKind::Escudo => &mut self.escudo,
            ItemKind::Pocao => &mut self.pocao,
        }
    }
}

/// Mochila: item + quantidade. `Vec` (não `HashMap`) porque a ordem de
/// exibição no Grimório importa e a lista é pequena — sem necessidade de
/// busca por chave em tempo de jogo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bag(pub Vec<(Item, u32)>);

/// Classe do jogador (RFC-003 §1). Não restringe equipamento (não-objetivo
/// 1 da RFC) — só decide qual `ItemKind` recebe `CLASS_BONUS_DAMAGE`
/// (`script/api.rs`) quando usado em `atacar()` (`resolve_attack`,
/// `script/vm.rs`). Escolhida em `scenes/grimoire.rs`, nunca via
/// pseudo-código (não-objetivo 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerClass {
    Guerreiro,
    Mago,
    Ladrao,
}

impl PlayerClass {
    /// O `ItemKind` que ganha o bônus de classe quando usado em
    /// `atacar()`. `match` exaustivo: uma classe nova sem afinidade
    /// definida quebra a compilação aqui, não em silêncio (mesmo padrão de
    /// `Loadout::slot`).
    ///
    /// `Ladrao -> Pocao` (não `Escudo`): decisão de tema — ladrão depende
    /// de truques/consumíveis, não de bloqueio direto (RFC-003).
    pub fn affinity(self) -> ItemKind {
        match self {
            PlayerClass::Guerreiro => ItemKind::Espada,
            PlayerClass::Mago => ItemKind::Magia,
            PlayerClass::Ladrao => ItemKind::Pocao,
        }
    }

    /// As três classes, na ordem em que aparecem no seletor do Grimório.
    pub const ALL: [PlayerClass; 3] = [PlayerClass::Guerreiro, PlayerClass::Mago, PlayerClass::Ladrao];

    /// Nome de exibição em português — usado só pela UI do Grimório.
    pub fn label(self) -> &'static str {
        match self {
            PlayerClass::Guerreiro => "GUERREIRO",
            PlayerClass::Mago => "MAGO",
            PlayerClass::Ladrao => "LADRAO",
        }
    }
}

/// Um script salvo pelo jogador na tela de duelo (RFC-002, regra 10).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SavedScript {
    pub name: String,
    pub body: String,
}

/// Estado persistente do jogador inteiro — uma única unidade lógica
/// serializada num único arquivo JSON (RFC-002, regra 3). Não modela
/// progressão de câmara/campanha (RFC-005 não antecipada): "salvar" aqui é
/// só "lembrar inventário e scripts entre execuções do jogo".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub loadout: Loadout,
    pub bag: Bag,
    pub scripts: Vec<SavedScript>,
    /// Classe escolhida no Grimório (RFC-003 §1). `#[serde(default)]`:
    /// um save gravado antes desta RFC não tem esse campo no JSON —
    /// desserializa como `None` em vez de falhar (mesmo espírito da regra
    /// "item ausente nunca é erro").
    #[serde(default)]
    pub player_class: Option<PlayerClass>,
    /// Índice da próxima fase a enfrentar (RFC-005 regra 2). `0` = primeira
    /// fase. `#[serde(default)]`: um save gravado antes desta RFC não tem
    /// essa chave no JSON — desserializa como `0`, não falha (mesmo padrão
    /// de `player_class` acima). `>= monsters::PHASES.len()` (7 hoje)
    /// significa "todas as fases vencidas" — vitória completa da pirâmide.
    #[serde(default)]
    pub current_phase: usize,
    /// Vida do jogador persistida entre fases (RFC-025, regra 5) — antes
    /// desta RFC, `PhaseScene::new` (`scenes/phase.rs`) recriava o jogador
    /// com vida cheia a cada fase (causa 2 de
    /// `ANALISE-por-que-o-jogo-e-facil.md`), zerando qualquer risco
    /// acumulado da campanha. `None` = vida cheia — mesmo valor tanto para
    /// um save gravado antes desta RFC (sem esta chave no JSON) quanto
    /// para o começo de uma expedição nova, `#[serde(default)]` no mesmo
    /// padrão de `player_class`/`current_phase` acima.
    #[serde(default)]
    pub player_life: Option<i32>,
}

/// RFC-025 regra 6: fração da vida perdida que o jogador recupera ao
/// vencer uma fase — não é cura total (isso zeraria o desgaste que esta
/// RFC inteira existe para criar) nem é recuperação zero (isso tornaria a
/// campanha de 7 fases praticamente impossível de terminar viva, morte por
/// acumulação aritmética pura, o que a RFC explicitamente não quer:
/// "a intenção é desgaste, não morte inevitável"). Calibrado pelos dois
/// testes de campanha permanentes de `script::vm::tests`
/// (`campanha_bem_jogada_sobrevive`/`campanha_mal_jogada_morre`, RFC-025
/// regra 8) — 90% é a fração medida que deixa a campanha bem jogada
/// terminar com margem real (14 de 100 de vida no 7º monstro), sem
/// eliminar o risco: a campanha mal jogada morre já na 1ª fase, mesmo com
/// a mesma recuperação entre fases (não dá tempo de a fração importar).
pub const PLAYER_LIFE_RECOVERY_NUM: i32 = 9;
pub const PLAYER_LIFE_RECOVERY_DEN: i32 = 10;

/// Aplica a recuperação parcial (regra 6) sobre a vida com que o jogador
/// terminou a fase que acabou de vencer. Nunca ultrapassa `max` (sem
/// overheal) nem produz um valor menor que `current` (a fórmula só
/// recupera vida perdida, nunca subtrai).
pub fn recovered_player_life(current: i32, max: i32) -> i32 {
    let missing = (max - current).max(0);
    let recovered = missing * PLAYER_LIFE_RECOVERY_NUM / PLAYER_LIFE_RECOVERY_DEN;
    (current + recovered).min(max)
}

const SAVE_FILE_NAME: &str = "piiramid_save.json";

/// Caminho do arquivo de save: ao lado do binário em execução (regra 3).
/// Se por algum motivo o executável não tiver diretório resolvível (não
/// deveria acontecer em nenhuma plataforma suportada), cai para o
/// diretório de trabalho atual — nunca é razão para crashar.
fn save_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
        .unwrap_or_default()
        .join(SAVE_FILE_NAME)
}

impl SaveData {
    /// Carrega o save do disco. Arquivo ausente, ilegível, ou com JSON
    /// inválido/de formato antigo **nunca** faz o jogo cair — todos esses
    /// casos caem silenciosamente para `SaveData::default()`, o mesmo
    /// estado de "Nova Expedição" (RFC-002, critério de aceite: save
    /// corrompido não crasha).
    pub fn load() -> SaveData {
        Self::load_from(&save_path())
    }

    fn load_from(path: &PathBuf) -> SaveData {
        std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    }

    /// Grava o save no disco. Falha de escrita (disco cheio, permissão)
    /// é engolida de propósito: perder o save é ruim, mas travar o jogo
    /// por isso seria pior — mesmo espírito da regra "nunca crasha".
    pub fn save(&self) {
        let _ = self.save_to(&save_path());
    }

    fn save_to(&self, path: &PathBuf) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveData {
        SaveData {
            loadout: Loadout {
                arma: Some(Item { id: "khopesh_trincado".into(), kind: ItemKind::Espada, name: "ferro".into(), bonus_damage: 6 }),
                magia: None,
                escudo: Some(Item { id: "bronze_lascado".into(), kind: ItemKind::Escudo, name: "bronze".into(), bonus_damage: 0 }),
                pocao: None,
            },
            bag: Bag(vec![
                (Item { id: "seiva_de_lotus".into(), kind: ItemKind::Pocao, name: "vida".into(), bonus_damage: 0 }, 3),
                (Item { id: "chama_do_oasis".into(), kind: ItemKind::Magia, name: "fogo".into(), bonus_damage: 8 }, 1),
            ]),
            scripts: vec![SavedScript { name: "abre-fogo.pii".into(), body: "atacar(magia.Fogo)\ndefender(escudo.Bronze)".into() }],
            player_class: None,
            current_phase: 0,
            player_life: None,
        }
    }

    #[test]
    fn round_trip_serializes_and_deserializes_without_loss() {
        let data = sample();
        let json = serde_json::to_string(&data).expect("serializar SaveData");
        let back: SaveData = serde_json::from_str(&json).expect("desserializar SaveData");
        assert_eq!(data, back);
    }

    #[test]
    fn item_kind_round_trips_by_label() {
        for kind in [ItemKind::Espada, ItemKind::Magia, ItemKind::Escudo, ItemKind::Pocao] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.label()));
            let back: ItemKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn loadout_slot_matches_item_kind_exhaustively() {
        let mut lo = Loadout::default();
        *lo.slot_mut(ItemKind::Escudo) = Some(Item { id: "x".into(), kind: ItemKind::Escudo, name: "bronze".into(), bonus_damage: 3 });
        assert_eq!(lo.slot(ItemKind::Escudo).map(|i| i.bonus_damage), Some(3));
        assert_eq!(lo.slot(ItemKind::Espada), None);
    }

    #[test]
    fn load_from_missing_file_falls_back_to_default_never_panics() {
        let path = PathBuf::from("/definitely/does/not/exist/piiramid_save_test.json");
        assert_eq!(SaveData::load_from(&path), SaveData::default());
    }

    #[test]
    fn load_from_corrupted_json_falls_back_to_default_never_panics() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("piiramid_corrupt_test_{}.json", std::process::id()));
        std::fs::write(&path, b"{ isto nao e json valido").unwrap();
        let loaded = SaveData::load_from(&path);
        assert_eq!(loaded, SaveData::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn player_class_affinity_is_exhaustive_and_matches_rfc_003() {
        assert_eq!(PlayerClass::Guerreiro.affinity(), ItemKind::Espada);
        assert_eq!(PlayerClass::Mago.affinity(), ItemKind::Magia);
        assert_eq!(PlayerClass::Ladrao.affinity(), ItemKind::Pocao);
    }

    #[test]
    fn save_data_with_player_class_round_trips() {
        let mut data = sample();
        data.player_class = Some(PlayerClass::Mago);
        let json = serde_json::to_string(&data).expect("serializar SaveData com classe");
        let back: SaveData = serde_json::from_str(&json).expect("desserializar SaveData com classe");
        assert_eq!(back.player_class, Some(PlayerClass::Mago));
        assert_eq!(data, back);
    }

    #[test]
    fn save_json_without_player_class_field_deserializes_as_none() {
        // Simula um save gravado antes da RFC-003: o JSON nao tem a chave
        // "player_class" -- #[serde(default)] precisa cobrir esse caso sem
        // falhar (regra 2 da RFC-003).
        let json = r#"{"loadout":{"arma":null,"magia":null,"escudo":null,"pocao":null},"bag":[],"scripts":[]}"#;
        let loaded: SaveData = serde_json::from_str(json).expect("save antigo sem player_class deve desserializar");
        assert_eq!(loaded.player_class, None);
    }

    #[test]
    fn default_save_starts_at_phase_zero() {
        assert_eq!(SaveData::default().current_phase, 0);
    }

    #[test]
    fn save_json_without_current_phase_field_deserializes_as_zero() {
        // Simula um save gravado antes da RFC-005: o JSON nao tem a chave
        // "current_phase" -- #[serde(default)] precisa cobrir esse caso sem
        // falhar (regra 2 da RFC-005), mesmo padrao de player_class acima.
        let json = r#"{"loadout":{"arma":null,"magia":null,"escudo":null,"pocao":null},"bag":[],"scripts":[],"player_class":null}"#;
        let loaded: SaveData = serde_json::from_str(json).expect("save antigo sem current_phase deve desserializar");
        assert_eq!(loaded.current_phase, 0);
    }

    #[test]
    fn save_data_with_current_phase_round_trips() {
        let mut data = sample();
        data.current_phase = 3;
        let json = serde_json::to_string(&data).expect("serializar SaveData com fase");
        let back: SaveData = serde_json::from_str(&json).expect("desserializar SaveData com fase");
        assert_eq!(back.current_phase, 3);
        assert_eq!(data, back);
    }

    #[test]
    fn default_save_has_full_life() {
        assert_eq!(SaveData::default().player_life, None);
    }

    #[test]
    fn save_json_without_player_life_field_deserializes_as_none() {
        // Simula um save gravado antes da RFC-025: o JSON nao tem a chave
        // "player_life" -- #[serde(default)] precisa cobrir esse caso sem
        // falhar (regra 5 da RFC-025), mesmo padrao de current_phase acima.
        let json = r#"{"loadout":{"arma":null,"magia":null,"escudo":null,"pocao":null},"bag":[],"scripts":[],"player_class":null,"current_phase":2}"#;
        let loaded: SaveData = serde_json::from_str(json).expect("save antigo sem player_life deve desserializar");
        assert_eq!(loaded.player_life, None);
    }

    #[test]
    fn save_data_with_player_life_round_trips() {
        let mut data = sample();
        data.player_life = Some(42);
        let json = serde_json::to_string(&data).expect("serializar SaveData com vida");
        let back: SaveData = serde_json::from_str(&json).expect("desserializar SaveData com vida");
        assert_eq!(back.player_life, Some(42));
        assert_eq!(data, back);
    }

    #[test]
    fn recovered_player_life_is_partial_never_full_never_less_than_current() {
        // RFC-025 regra 6: recupera uma fracao da vida perdida, nunca cura
        // total (senao o desgaste desapareceria) nem recupera menos que
        // zero (a formula so soma vida perdida, nunca subtrai).
        assert_eq!(recovered_player_life(10, 100), 10 + (90 * PLAYER_LIFE_RECOVERY_NUM / PLAYER_LIFE_RECOVERY_DEN));
        assert!(recovered_player_life(10, 100) < 100, "recuperacao parcial nunca pode ser cura total");
        assert!(recovered_player_life(10, 100) > 10, "recuperacao parcial precisa recuperar alguma vida");
        assert_eq!(recovered_player_life(100, 100), 100, "vida cheia nao pode overhealar acima do maximo");
        assert_eq!(recovered_player_life(0, 100), 90, "90% de recuperacao sobre 100 de vida perdida");
    }

    #[test]
    fn save_then_load_round_trips_through_a_real_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("piiramid_save_roundtrip_test_{}.json", std::process::id()));
        let data = sample();
        data.save_to(&path).expect("gravar save de teste");
        let loaded = SaveData::load_from(&path);
        assert_eq!(loaded, data);
        let _ = std::fs::remove_file(&path);
    }
}

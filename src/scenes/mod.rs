pub mod duel;
pub mod gameover;
pub mod grimoire;
pub mod intro;
pub mod menu;
pub mod overworld;
pub mod phase;
pub mod style_guide;

/// O que uma cena pede ao `main.rs` para fazer em seguida.
pub enum Transition {
    /// RFC-002: carrega o `SaveData` decidido no menu ("Nova Expedição" ->
    /// `SaveData::default()`, "Continuar" -> `SaveData::load()`) — mesmo
    /// padrão que `GoToGameOver` já usa pra passar dados entre cenas.
    /// RFC-005: só alcançável via o item de menu condicional a
    /// `#[cfg(debug_assertions)]` — deixa de ser o caminho padrão.
    GoToOverworld { save: Box<crate::inventory::SaveData> },
    /// RFC-005 regra 3: fluxo padrão do menu. Monta o duelo da fase
    /// `save.current_phase` (via `monsters::PHASES`) direto, sem
    /// mapa/movimento — mesmo padrão de `save` emprestado que `GoToOverworld`
    /// já usa.
    GoToPhase { save: Box<crate::inventory::SaveData> },
    /// RFC-023 regra 4: só alcançável por "NOVA EXPEDICAO"
    /// (`MenuAction::Phase { fresh: true }`) — "CONTINUAR" nunca passa por
    /// aqui, vai direto para `GoToPhase`. A introdução em si dispara
    /// `GoToPhase { save }` ao terminar ou ser pulada (regra 7), com o
    /// mesmo `save`, intacto.
    GoToIntro { save: Box<crate::inventory::SaveData> },
    GoToGameOver { won: bool, turns: u32, player_hp: i32 },
    GoToMenu,
    GoToGrimoire,
    GoToStyleGuide,
    Quit,
}

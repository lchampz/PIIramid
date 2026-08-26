pub mod duel;
pub mod gameover;
pub mod grimoire;
pub mod menu;
pub mod overworld;
pub mod style_guide;

/// O que uma cena pede ao `main.rs` para fazer em seguida.
pub enum Transition {
    GoToOverworld,
    GoToGameOver { won: bool, turns: u32, player_hp: i32 },
    GoToMenu,
    GoToGrimoire,
    GoToStyleGuide,
    Quit,
}

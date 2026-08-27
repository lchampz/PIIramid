//! O núcleo novo do jogo: o interpretador de pseudo-código do duelo.
//! `lexer` -> `parser` -> `ast`, executado por `vm` com a tabela de custos
//! de `api`. Tudo aqui é lógica pura, sem macroquad — testável só com
//! `cargo test`.

pub mod api;
pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod rehearsal;
pub mod value;
pub mod vm;

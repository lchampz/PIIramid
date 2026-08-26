//! Erro de script, sempre com a linha onde ocorreu — é o que permite à
//! cena de duelo apontar exatamente a linha errada no editor sem consumir
//! o turno do jogador.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptError {
    pub line: usize,
    pub message: String,
}

impl ScriptError {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        ScriptError { line, message: message.into() }
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "linha {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ScriptError {}

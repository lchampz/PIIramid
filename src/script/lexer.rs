//! Tokenizador do pseudo-código do duelo.
//!
//! Produz NEWLINE/INDENT/DEDENT ao estilo Python para o estilo de bloco por
//! indentação, mas os **suprime** enquanto há parênteses, colchetes ou
//! chaves abertos — isso é o que permite o estilo `{}` conviver com o
//! estilo `:` no mesmo script: dentro de `{...}` a indentação vira só
//! estética, e quem separa comandos é o parser reconhecendo o início de
//! cada statement (mais `;` opcional).
//!
//! O aninhamento funciona em uma direção: um bloco `{}` pode aparecer
//! dentro de um bloco por indentação (a indentação continua sendo medida
//! normalmente fora das chaves), mas não o contrário — uma vez dentro de
//! `{...}`, a indentação para de ter significado estrutural até a chave
//! fechar, então um sub-bloco `:`/indentado não pode viver lá dentro.

use super::error::ScriptError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Newline,
    Indent,
    Dedent,
    Eof,

    Ident(String),
    Number(f64),
    Str(String),
    True,
    False,

    If,
    Else,
    While,
    For,
    In,
    Func,
    Invocar,
    /// `selecionar` (RFC-015) — gramática fixa `selecionar(mochila, onde:
    /// <expr>, limite: <expr>)`. `onde`/`limite`/`mochila` continuam
    /// identificadores comuns, só `selecionar` é keyword de verdade.
    Selecionar,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    Assign,
    EqEq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,

    And,
    Or,
    Not,

    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,

    Colon,
    Comma,
    Dot,
    DotDot,
    Semicolon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokKind,
    pub line: usize,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, ScriptError> {
    Lexer::new(src).run()
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    /// profundidade combinada de (), [] e {}
    bracket_depth: i32,
    indent_stack: Vec<usize>,
    at_line_start: bool,
    tokens: Vec<Token>,
}

impl Lexer {
    fn new(src: &str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            bracket_depth: 0,
            indent_stack: vec![0],
            at_line_start: true,
            tokens: Vec::new(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn push(&mut self, kind: TokKind) {
        let line = self.line;
        self.tokens.push(Token { kind, line });
    }

    fn run(mut self) -> Result<Vec<Token>, ScriptError> {
        loop {
            if self.at_line_start && self.bracket_depth == 0 && self.consume_indentation()? {
                // linha em branco ou comentário puro: já avançou até o fim da linha
                continue;
            }

            match self.peek() {
                None => break,
                Some('\n') => {
                    self.advance();
                    if self.bracket_depth == 0 {
                        self.push(TokKind::Newline);
                        self.at_line_start = true;
                    }
                    self.line += 1;
                }
                Some(c) if c == ' ' || c == '\t' || c == '\r' => {
                    self.advance();
                }
                Some('#') => self.skip_line_comment(),
                Some('/') if self.peek_at(1) == Some('/') => self.skip_line_comment(),
                Some(c) if c.is_ascii_digit() => self.lex_number()?,
                Some(c) if is_ident_start(c) => self.lex_ident(),
                Some('"') => self.lex_string()?,
                Some(_) => self.lex_operator()?,
            }
        }

        // Fecha a última linha lógica com um `Newline` sintético — mas só
        // se nenhum `(`/`[`/`{` ficou pendente. Enquanto `bracket_depth >
        // 0` o `Newline` é suprimido em todo o resto do lexer (é o que
        // permite uma chamada continuar em várias linhas); esse flush de
        // fim de arquivo era a única exceção que ignorava a mesma regra,
        // e é isso que causava o bug B-004: um `Newline` fantasma surgia
        // bem onde o parser esperava o token de fechamento, empurrando o
        // erro "token inesperado" para a linha física do fim do arquivo
        // (ou de uma linha em branco deixada por um Enter) em vez de deixar
        // o parser reconhecer que é um EOF puro e reportar a linha de
        // abertura do delimitador (ver `Parser::expect_closing`).
        if !self.at_line_start && self.bracket_depth == 0 {
            self.push(TokKind::Newline);
        }
        // Mesma lógica vale para o `Dedent` de fechamento: só faz sentido
        // fechar blocos por indentação pendentes se nenhum delimitador
        // ficou aberto. Se `bracket_depth > 0` no EOF, o script já está
        // com um erro de sintaxe mais direto (delimitador nunca fechado) —
        // emitir `Dedent`(s) aqui só faria o parser tropeçar num token
        // estrutural inesperado (`token inesperado: Dedent`) na mesma linha
        // inflada, em vez de deixar `Parser::expect_closing` reportar a
        // linha de abertura do delimitador.
        if self.bracket_depth == 0 {
            while self.indent_stack.len() > 1 {
                self.indent_stack.pop();
                self.push(TokKind::Dedent);
            }
        }
        self.push(TokKind::Eof);

        Ok(self.tokens)
    }

    /// Mede a indentação de uma nova linha lógica e emite Indent/Dedent.
    /// Retorna Ok(true) se a linha era em branco ou só comentário (e já foi
    /// consumida por inteiro, sem afetar a pilha de indentação).
    fn consume_indentation(&mut self) -> Result<bool, ScriptError> {
        let mut indent = 0usize;
        let mut i = self.pos;
        while let Some(&c) = self.chars.get(i) {
            match c {
                ' ' => indent += 1,
                '\t' => indent += 4,
                _ => break,
            }
            i += 1;
        }

        match self.chars.get(i) {
            None => {
                // fim do arquivo: não há mais linha nenhuma para medir.
                // Retorna false (nada tratado) para o loop principal cair
                // no `match self.peek() { None => break, .. }` — devolver
                // true aqui faria o chamador dar `continue` para sempre,
                // já que `at_line_start` nunca seria desarmado.
                self.pos = i;
                return Ok(false);
            }
            Some('\n') => {
                self.pos = i + 1;
                self.line += 1;
                return Ok(true);
            }
            Some('#') => {
                self.pos = i;
                self.skip_line_comment();
                return Ok(true);
            }
            Some('/') if self.chars.get(i + 1) == Some(&'/') => {
                self.pos = i;
                self.skip_line_comment();
                return Ok(true);
            }
            _ => {}
        }

        self.pos = i;
        self.at_line_start = false;

        let current = *self.indent_stack.last().unwrap();
        if indent > current {
            self.indent_stack.push(indent);
            self.push(TokKind::Indent);
        } else if indent < current {
            while indent < *self.indent_stack.last().unwrap() {
                self.indent_stack.pop();
                self.push(TokKind::Dedent);
            }
            if *self.indent_stack.last().unwrap() != indent {
                return Err(ScriptError::new(self.line, "indentacao inconsistente"));
            }
        }
        Ok(false)
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn lex_number(&mut self) -> Result<(), ScriptError> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.advance();
        }
        if self.peek() == Some('.') && self.peek_at(1) != Some('.') {
            self.advance();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.advance();
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let value: f64 = text
            .parse()
            .map_err(|_| ScriptError::new(self.line, format!("numero invalido: '{text}'")))?;
        self.push(TokKind::Number(value));
        Ok(())
    }

    fn lex_ident(&mut self) {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if is_ident_continue(c)) {
            self.advance();
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let kind = match text.as_str() {
            "if" => TokKind::If,
            "else" => TokKind::Else,
            "while" => TokKind::While,
            "for" => TokKind::For,
            "in" => TokKind::In,
            "func" => TokKind::Func,
            "invocar" => TokKind::Invocar,
            "selecionar" => TokKind::Selecionar,
            "and" | "e" => TokKind::And,
            "or" | "ou" => TokKind::Or,
            "not" | "nao" => TokKind::Not,
            "true" | "verdadeiro" => TokKind::True,
            "false" | "falso" => TokKind::False,
            _ => TokKind::Ident(text),
        };
        self.push(kind);
    }

    fn lex_string(&mut self) -> Result<(), ScriptError> {
        let line = self.line;
        self.advance(); // consome a aspas de abertura
        let mut value = String::new();
        loop {
            match self.advance() {
                None => return Err(ScriptError::new(line, "string nao fechada")),
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some(other) => value.push(other),
                    None => return Err(ScriptError::new(line, "string nao fechada")),
                },
                Some(c) => value.push(c),
            }
        }
        self.push(TokKind::Str(value));
        Ok(())
    }

    fn lex_operator(&mut self) -> Result<(), ScriptError> {
        let c = self.advance().unwrap();
        let kind = match c {
            '+' => TokKind::Plus,
            '-' => TokKind::Minus,
            '*' => TokKind::Star,
            '/' => TokKind::Slash,
            '%' => TokKind::Percent,
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokKind::EqEq
                } else {
                    TokKind::Assign
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokKind::NotEq
                } else {
                    TokKind::Not
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokKind::Le
                } else {
                    TokKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokKind::Ge
                } else {
                    TokKind::Gt
                }
            }
            '(' => {
                self.bracket_depth += 1;
                TokKind::LParen
            }
            ')' => {
                self.bracket_depth = (self.bracket_depth - 1).max(0);
                TokKind::RParen
            }
            '[' => {
                self.bracket_depth += 1;
                TokKind::LBracket
            }
            ']' => {
                self.bracket_depth = (self.bracket_depth - 1).max(0);
                TokKind::RBracket
            }
            '{' => {
                self.bracket_depth += 1;
                TokKind::LBrace
            }
            '}' => {
                self.bracket_depth = (self.bracket_depth - 1).max(0);
                TokKind::RBrace
            }
            ':' => TokKind::Colon,
            ',' => TokKind::Comma,
            ';' => TokKind::Semicolon,
            '.' => {
                if self.peek() == Some('.') {
                    self.advance();
                    TokKind::DotDot
                } else {
                    TokKind::Dot
                }
            }
            other => return Err(ScriptError::new(self.line, format!("caractere inesperado: '{other}'"))),
        };
        self.push(kind);
        Ok(())
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn simple_call() {
        assert_eq!(
            kinds("atacar()"),
            vec![
                TokKind::Ident("atacar".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn indent_dedent_around_if() {
        let toks = kinds("if x:\n    atacar()\ny()\n");
        assert_eq!(
            toks,
            vec![
                TokKind::If,
                TokKind::Ident("x".into()),
                TokKind::Colon,
                TokKind::Newline,
                TokKind::Indent,
                TokKind::Ident("atacar".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Dedent,
                TokKind::Ident("y".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn braces_suppress_newlines() {
        let toks = kinds("if x {\n    atacar()\n}\n");
        // dentro de {} não deve haver Indent/Dedent/Newline algum
        assert!(!toks.contains(&TokKind::Indent));
        assert!(!toks.contains(&TokKind::Dedent));
        assert_eq!(toks.iter().filter(|k| **k == TokKind::Newline).count(), 1);
    }

    #[test]
    fn blank_lines_and_comments_ignored() {
        let toks = kinds("atacar()\n\n# comentario\n// outro\ndefender()\n");
        assert_eq!(
            toks,
            vec![
                TokKind::Ident("atacar".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Ident("defender".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn string_and_index() {
        let toks = kinds("escudo[\"ouro\"]");
        assert_eq!(
            toks,
            vec![
                TokKind::Ident("escudo".into()),
                TokKind::LBracket,
                TokKind::Str("ouro".into()),
                TokKind::RBracket,
                TokKind::Newline,
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn inconsistent_indent_errors() {
        let err = tokenize("if x:\n    atacar()\n  defender()\n").unwrap_err();
        assert!(err.message.contains("indentacao"));
    }

    #[test]
    fn func_keyword_is_recognized() {
        let toks = kinds("func combo():\n    atacar()\n");
        assert_eq!(
            toks,
            vec![
                TokKind::Func,
                TokKind::Ident("combo".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Colon,
                TokKind::Newline,
                TokKind::Indent,
                TokKind::Ident("atacar".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::Newline,
                TokKind::Dedent,
                TokKind::Eof,
            ]
        );
    }
}

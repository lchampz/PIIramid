//! Editor de texto multi-linha usado na tela de duelo para escrever o
//! script em pseudo-código. É o único componente de UI que não existia
//! no jogo original — o confront.c só lia dígitos de 0-9.
//!
//! Recursos: cursor, Enter/Backspace/setas, auto-indent depois de ":" ou
//! "{", e desindent automático ao digitar "}" no início de uma linha.

use macroquad::prelude::*;

const INDENT: &str = "    ";

pub struct CodeEditor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    /// linha atualmente destacada durante a execução animada (0-based), se houver
    pub highlighted_line: Option<usize>,
}

impl Default for CodeEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeEditor {
    pub fn new() -> Self {
        CodeEditor { lines: vec![String::new()], cursor_row: 0, cursor_col: 0, highlighted_line: None }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.highlighted_line = None;
    }

    /// RFC-026 regra 2: substitui todo o conteúdo do editor pelo `body` de
    /// um `SavedScript` carregado — sempre substitui, nunca funde com o que
    /// já estava no editor (a RFC pede "sem merge/append" explicitamente).
    /// Cursor vai para o fim do texto carregado, mesmo espírito de abrir um
    /// arquivo existente num editor de texto comum.
    pub fn load_text(&mut self, text: &str) {
        self.lines = if text.is_empty() { vec![String::new()] } else { text.split('\n').map(str::to_string).collect() };
        self.cursor_row = self.lines.len() - 1;
        self.cursor_col = self.lines[self.cursor_row].chars().count();
        self.highlighted_line = None;
    }

    /// Insere um trecho de texto (pode ter múltiplas linhas) na posição do
    /// cursor, como se tivesse sido digitado — usado pela paleta de
    /// comandos clicável para inserir uma chamada pronta.
    pub fn insert_snippet(&mut self, text: &str) {
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                self.insert_newline();
            }
            for ch in part.chars() {
                self.insert_char(ch);
            }
        }
    }

    fn current_line_mut(&mut self) -> &mut String {
        &mut self.lines[self.cursor_row]
    }

    fn indent_of(line: &str) -> String {
        line.chars().take_while(|c| *c == ' ').collect()
    }

    /// Processa entrada de teclado deste frame. Deve ser chamado uma vez por
    /// frame enquanto o editor está focado.
    pub fn update(&mut self) {
        while let Some(c) = get_char_pressed() {
            if c.is_control() {
                continue;
            }
            self.insert_char(c);
        }

        if is_key_pressed(KeyCode::Enter) {
            self.insert_newline();
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.backspace();
        }
        if is_key_pressed(KeyCode::Tab) {
            for ch in INDENT.chars() {
                self.insert_char(ch);
            }
        }
        if is_key_pressed(KeyCode::Left) {
            self.move_left();
        }
        if is_key_pressed(KeyCode::Right) {
            self.move_right();
        }
        if is_key_pressed(KeyCode::Up) && self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].chars().count());
        }
        if is_key_pressed(KeyCode::Down) && self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].chars().count());
        }
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
        }
    }

    fn move_right(&mut self) {
        let len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn insert_char(&mut self, c: char) {
        // desindenta automaticamente ao fechar chave no início da linha
        if c == '}' {
            let line = self.lines[self.cursor_row].clone();
            let before_cursor: String = line.chars().take(self.cursor_col).collect();
            if before_cursor.chars().all(|ch| ch == ' ') && before_cursor.len() >= INDENT.len() {
                let new_indent_len = before_cursor.len() - INDENT.len();
                let rest: String = line.chars().skip(self.cursor_col).collect();
                let mut new_line = " ".repeat(new_indent_len);
                new_line.push('}');
                new_line.push_str(&rest);
                self.lines[self.cursor_row] = new_line;
                self.cursor_col = new_indent_len + 1;
                return;
            }
        }

        let col = self.cursor_col;
        let line = self.current_line_mut();
        let mut chars: Vec<char> = line.chars().collect();
        chars.insert(col, c);
        *line = chars.into_iter().collect();
        self.cursor_col += 1;
    }

    fn insert_newline(&mut self) {
        let line = self.lines[self.cursor_row].clone();
        let chars: Vec<char> = line.chars().collect();
        let before: String = chars[..self.cursor_col].iter().collect();
        let after: String = chars[self.cursor_col..].iter().collect();

        let trimmed = before.trim_end();
        let opens_block = trimmed.ends_with(':') || trimmed.ends_with('{');

        let mut indent = Self::indent_of(&before);
        if opens_block {
            indent.push_str(INDENT);
        }

        self.lines[self.cursor_row] = before;
        let new_line = format!("{indent}{after}");
        self.lines.insert(self.cursor_row + 1, new_line);
        self.cursor_row += 1;
        self.cursor_col = indent.chars().count();
    }

    /// RFC-033 regra 1: caractere de identificador para fins de
    /// autocomplete -- mesmo critério de `is_ident_continue` do lexer
    /// (`script/lexer.rs`), duplicado aqui de propósito: este módulo é UI
    /// pura (`macroquad`) e o lexer é lógica pura sem `macroquad` (fronteira
    /// intocável, ver `[[gamedev]]`), então os dois lados não podem
    /// compartilhar a função sem violar a separação.
    fn is_ident_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// RFC-033 regra 1: se o cursor está no meio ou no fim de uma sequência
    /// de caracteres de identificador, devolve `(coluna onde a sequência
    /// começa, o texto já digitado antes do cursor)` -- é esse par que o
    /// autocomplete usa como âncora (onde substituir) e prefixo (o que
    /// filtrar). Devolve `None` quando o caractere imediatamente antes do
    /// cursor não é de identificador (início de linha, depois de espaço,
    /// depois de `(`/`.`/etc.) -- nesse caso não há prefixo nenhum para
    /// completar.
    ///
    /// Só olha para a linha atual e nunca distingue string literal ou
    /// comentário de código normal (o editor não faz essa distinção hoje —
    /// dívida registrada na entrega da RFC-033, não é bloqueio dela).
    pub fn identifier_prefix_before_cursor(&self) -> Option<(usize, String)> {
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        if self.cursor_col == 0 || !Self::is_ident_char(chars[self.cursor_col - 1]) {
            return None;
        }
        let mut start = self.cursor_col;
        while start > 0 && Self::is_ident_char(chars[start - 1]) {
            start -= 1;
        }
        let prefix: String = chars[start..self.cursor_col].iter().collect();
        Some((start, prefix))
    }

    /// RFC-033 regra 2: substitui só o trecho `[start_col, cursor_col)` da
    /// linha atual (o prefixo que `identifier_prefix_before_cursor` mediu)
    /// pelo `replacement` completo, e move o cursor para o fim do texto
    /// inserido -- o resto da linha, incluindo qualquer caractere de
    /// identificador que já estivesse depois do cursor, nunca é tocado
    /// ("mantém o resto da linha intacto", regra 2 da RFC).
    pub fn replace_identifier_prefix(&mut self, start_col: usize, replacement: &str) {
        let row = self.cursor_row;
        let chars: Vec<char> = self.lines[row].chars().collect();
        let mut new_chars: Vec<char> = chars[..start_col].to_vec();
        new_chars.extend(replacement.chars());
        new_chars.extend(chars[self.cursor_col..].iter().copied());
        self.lines[row] = new_chars.into_iter().collect();
        self.cursor_col = start_col + replacement.chars().count();
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let col = self.cursor_col;
            let line = self.current_line_mut();
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(col - 1);
            *line = chars.into_iter().collect();
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            let removed = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
            self.lines[self.cursor_row].push_str(&removed);
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> CodeEditor {
        CodeEditor::new()
    }

    #[test]
    fn typing_appends_to_line() {
        let mut ed = editor();
        for c in "atacar()".chars() {
            ed.insert_char(c);
        }
        assert_eq!(ed.lines[0], "atacar()");
        assert_eq!(ed.cursor_col, 8);
    }

    #[test]
    fn enter_after_colon_indents() {
        let mut ed = editor();
        for c in "if x:".chars() {
            ed.insert_char(c);
        }
        ed.insert_newline();
        assert_eq!(ed.lines[1], "    ");
        assert_eq!(ed.cursor_col, 4);
    }

    #[test]
    fn enter_after_brace_indents() {
        let mut ed = editor();
        for c in "if x {".chars() {
            ed.insert_char(c);
        }
        ed.insert_newline();
        assert_eq!(ed.lines[1], "    ");
    }

    #[test]
    fn closing_brace_dedents() {
        let mut ed = editor();
        ed.lines[0] = "    ".to_string();
        ed.cursor_col = 4;
        ed.insert_char('}');
        assert_eq!(ed.lines[0], "}");
        assert_eq!(ed.cursor_col, 1);
    }

    #[test]
    fn load_text_replaces_content_and_moves_cursor_to_end() {
        let mut ed = editor();
        ed.insert_snippet("velho()");
        ed.load_text("atacar(espada.Fogo)\ndefender(escudo.Bronze)");
        assert_eq!(ed.lines, vec!["atacar(espada.Fogo)".to_string(), "defender(escudo.Bronze)".to_string()]);
        assert_eq!(ed.cursor_row, 1);
        assert_eq!(ed.cursor_col, "defender(escudo.Bronze)".len());
    }

    #[test]
    fn identifier_prefix_detects_mid_word_and_end_of_word() {
        let mut ed = editor();
        ed.insert_snippet("atacar");
        // fim da palavra
        assert_eq!(ed.identifier_prefix_before_cursor(), Some((0, "atacar".to_string())));
        // meio da palavra ("ata|car")
        ed.cursor_col = 3;
        assert_eq!(ed.identifier_prefix_before_cursor(), Some((0, "ata".to_string())));
    }

    #[test]
    fn identifier_prefix_none_right_after_non_ident_char() {
        let mut ed = editor();
        ed.insert_snippet("atacar()");
        // cursor logo depois de ')' -- nao ha identificador imediatamente
        // antes do cursor
        assert_eq!(ed.identifier_prefix_before_cursor(), None);
    }

    #[test]
    fn identifier_prefix_none_at_start_of_line() {
        let ed = editor();
        assert_eq!(ed.identifier_prefix_before_cursor(), None);
    }

    #[test]
    fn replace_identifier_prefix_keeps_rest_of_line_intact() {
        let mut ed = editor();
        ed.insert_snippet("ata(espada.Fogo)");
        ed.cursor_col = 3; // "ata|(espada.Fogo)"
        ed.replace_identifier_prefix(0, "atacar");
        assert_eq!(ed.lines[0], "atacar(espada.Fogo)");
        assert_eq!(ed.cursor_col, 6);
    }

    #[test]
    fn backspace_merges_lines() {
        let mut ed = editor();
        ed.lines = vec!["ab".to_string(), "cd".to_string()];
        ed.cursor_row = 1;
        ed.cursor_col = 0;
        ed.backspace();
        assert_eq!(ed.lines, vec!["abcd".to_string()]);
        assert_eq!(ed.cursor_row, 0);
        assert_eq!(ed.cursor_col, 2);
    }
}

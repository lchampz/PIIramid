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

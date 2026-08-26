//! Parser descendente recursivo. A parte interessante é `block()`: ele
//! decide entre bloco por indentação (`:`) e bloco por chaves (`{}`)
//! olhando um único token à frente, e os dois produzem a mesma AST — os
//! testes no fim do arquivo comparam isso diretamente.

use super::ast::{BinOp, Expr, Stmt, StmtKind, UnaryOp};
use super::error::ScriptError;
use super::lexer::{tokenize, TokKind, Token};

pub fn parse(src: &str) -> Result<Vec<Stmt>, ScriptError> {
    let tokens = tokenize(src)?;
    Parser::new(tokens).parse_program()
}

/// Traduz um `TokKind` para a palavra que o jogador reconhece (bug B-001).
/// `{:?}` num `TokKind` imprime o nome da variante Rust — `RBrace`,
/// `Colon`, `Ident("atacar")` — util pra depurar o parser, inutil pra quem
/// so quer saber o que trocar no script. Cada braço devolve o que o
/// jogador de fato digitaria ou veria, nunca o nome interno do token.
/// `match` exaustivo de proposito: token novo no lexer sem braço aqui é
/// erro de compilacao, nao mensagem crua escapando de novo pro jogador.
fn describe_token(kind: &TokKind) -> String {
    match kind {
        TokKind::Newline => "fim de linha".to_string(),
        TokKind::Indent => "um aumento de indentacao".to_string(),
        TokKind::Dedent => "uma volta de indentacao".to_string(),
        TokKind::Eof => "fim do script".to_string(),
        TokKind::Ident(name) => format!("'{name}'"),
        TokKind::Number(n) => format!("o numero '{n}'"),
        TokKind::Str(s) => format!("o texto '{s}'"),
        TokKind::True => "'true'".to_string(),
        TokKind::False => "'false'".to_string(),
        TokKind::If => "'if'".to_string(),
        TokKind::Else => "'else'".to_string(),
        TokKind::While => "'while'".to_string(),
        TokKind::For => "'for'".to_string(),
        TokKind::In => "'in'".to_string(),
        TokKind::Func => "'func'".to_string(),
        TokKind::Invocar => "'invocar'".to_string(),
        TokKind::Selecionar => "'selecionar'".to_string(),
        TokKind::Plus => "'+'".to_string(),
        TokKind::Minus => "'-'".to_string(),
        TokKind::Star => "'*'".to_string(),
        TokKind::Slash => "'/'".to_string(),
        TokKind::Percent => "'%'".to_string(),
        TokKind::Assign => "'='".to_string(),
        TokKind::EqEq => "'=='".to_string(),
        TokKind::NotEq => "'!='".to_string(),
        TokKind::Lt => "'<'".to_string(),
        TokKind::Gt => "'>'".to_string(),
        TokKind::Le => "'<='".to_string(),
        TokKind::Ge => "'>='".to_string(),
        TokKind::And => "'and'".to_string(),
        TokKind::Or => "'or'".to_string(),
        TokKind::Not => "'not'".to_string(),
        TokKind::LParen => "'('".to_string(),
        TokKind::RParen => "')'".to_string(),
        TokKind::LBracket => "'['".to_string(),
        TokKind::RBracket => "']'".to_string(),
        TokKind::LBrace => "'{'".to_string(),
        TokKind::RBrace => "'}'".to_string(),
        TokKind::Colon => "':'".to_string(),
        TokKind::Comma => "','".to_string(),
        TokKind::Dot => "'.'".to_string(),
        TokKind::DotDot => "'..'".to_string(),
        TokKind::Semicolon => "';'".to_string(),
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Profundidade de blocos aninhados (corpo de `if`/`while`/`for`/`func`)
    /// em que o cursor está agora. `0` significa nível superior do script.
    /// É o que permite `parse_func` recusar `func` fora do nível superior
    /// (RFC-006, regra 5) sem duplicar a lógica de bloco: qualquer `block()`
    /// já aninha, então basta perguntar a profundidade atual.
    block_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, block_depth: 0 }
    }

    fn peek(&self) -> &TokKind {
        &self.tokens[self.pos].kind
    }

    fn peek_at(&self, offset: usize) -> &TokKind {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx].kind
    }

    fn line(&self) -> usize {
        self.tokens[self.pos].line
    }

    fn check(&self, kind: &TokKind) -> bool {
        self.peek() == kind
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: &TokKind, ctx: &str) -> Result<Token, ScriptError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(ScriptError::new(
                self.line(),
                format!("esperava {} em {ctx}, encontrei {}", describe_token(kind), describe_token(self.peek())),
            ))
        }
    }

    /// Como `expect`, mas para o fechamento de um delimitador (`)`, `]` ou
    /// `}`) cuja abertura ficou em `open_line` (bug B-004).
    ///
    /// Dentro de `()`/`[]`/`{}` o lexer suprime o token `Newline` mas
    /// continua contando linha física (`Lexer::line` em `lexer.rs`) — é o
    /// que permite uma chamada continuar em várias linhas. Se o delimitador
    /// nunca fecha, o token que sobra no fim (`Eof`) carrega a linha física
    /// onde o arquivo de fato termina, que já avançou por cima de toda
    /// quebra de linha engolida enquanto o delimitador estava aberto — não é
    /// a linha que ajuda o jogador a achar o erro, é a linha da última
    /// quebra de linha que ele digitou (inclusive uma linha vazia deixada
    /// por um Enter depois do erro). A linha útil é onde o delimitador abriu.
    /// Isso só se aplica quando o token restante é `Eof`: se o parser topou
    /// com um token concreto no meio do caminho (ex.: vírgula faltando), a
    /// linha desse token já é a correta e não deve ser substituída.
    fn expect_closing(&mut self, kind: &TokKind, open_line: usize, ctx: &str) -> Result<Token, ScriptError> {
        if self.check(kind) {
            Ok(self.advance())
        } else if self.check(&TokKind::Eof) {
            Err(ScriptError::new(
                open_line,
                format!("esperava {} em {ctx}, encontrei {}", describe_token(kind), describe_token(self.peek())),
            ))
        } else {
            Err(ScriptError::new(
                self.line(),
                format!("esperava {} em {ctx}, encontrei {}", describe_token(kind), describe_token(self.peek())),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ScriptError> {
        match self.peek().clone() {
            TokKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(ScriptError::new(
                self.line(),
                format!("esperava um identificador, encontrei {}", describe_token(&other)),
            )),
        }
    }

    /// Pula tokens Newline (mas nunca Dedent) — usado em pontos onde uma
    /// quebra de linha é apenas separadora e não estrutural, como entre um
    /// `}` de fechamento e um possível `else` na linha seguinte.
    fn skip_bare_newlines(&mut self) {
        while self.check(&TokKind::Newline) {
            self.advance();
        }
    }

    fn skip_seps(&mut self) {
        while self.check(&TokKind::Newline) || self.check(&TokKind::Semicolon) {
            self.advance();
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        let mut stmts = Vec::new();
        self.skip_seps();
        while !self.check(&TokKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_seps();
        }
        Ok(stmts)
    }

    fn parse_stmt_list_until(&mut self, end: &TokKind) -> Result<Vec<Stmt>, ScriptError> {
        let mut stmts = Vec::new();
        self.skip_seps();
        while !self.check(end) && !self.check(&TokKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_seps();
        }
        Ok(stmts)
    }

    /// `:` NEWLINE INDENT stmt+ DEDENT   |   `{` stmt* `}`
    ///
    /// Entrar em qualquer bloco (de `if`/`while`/`for`/`func`) incrementa
    /// `block_depth` enquanto o corpo é parseado — é assim que `parse_func`
    /// sabe recusar `func` que não esteja no nível superior (regra 5) sem
    /// duplicar a noção de "dentro de um bloco".
    fn block(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        self.block_depth += 1;
        let result = self.block_inner();
        self.block_depth -= 1;
        result
    }

    fn block_inner(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        if self.check(&TokKind::Colon) {
            self.advance();
            self.expect(&TokKind::Newline, "depois de ':'")?;
            self.expect(&TokKind::Indent, "inicio de bloco indentado")?;
            let body = self.parse_stmt_list_until(&TokKind::Dedent)?;
            self.expect(&TokKind::Dedent, "fim de bloco indentado")?;
            Ok(body)
        } else if self.check(&TokKind::LBrace) {
            let open_line = self.line();
            self.advance();
            let body = self.parse_stmt_list_until(&TokKind::RBrace)?;
            self.expect_closing(&TokKind::RBrace, open_line, "fim de bloco '{}'")?;
            Ok(body)
        } else {
            Err(ScriptError::new(
                self.line(),
                format!("esperava ':' ou '{{' para abrir um bloco, encontrei {}", describe_token(self.peek())),
            ))
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ScriptError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            TokKind::If => self.parse_if()?,
            TokKind::While => self.parse_while()?,
            TokKind::For => self.parse_for()?,
            TokKind::Func => self.parse_func()?,
            TokKind::Invocar => self.parse_invoke()?,
            TokKind::Ident(name) if *self.peek_at(1) == TokKind::Assign => {
                self.advance();
                self.advance(); // '='
                let value = self.parse_expr()?;
                StmtKind::Assign(name, value)
            }
            _ => StmtKind::Expr(self.parse_expr()?),
        };
        Ok(Stmt { line, kind })
    }

    fn parse_if(&mut self) -> Result<StmtKind, ScriptError> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then_branch = self.block()?;
        self.skip_bare_newlines();
        let else_branch = if self.check(&TokKind::Else) {
            self.advance();
            Some(self.block()?)
        } else {
            None
        };
        Ok(StmtKind::If { cond, then_branch, else_branch })
    }

    fn parse_while(&mut self) -> Result<StmtKind, ScriptError> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        let body = self.block()?;
        Ok(StmtKind::While { cond, body })
    }

    fn parse_for(&mut self) -> Result<StmtKind, ScriptError> {
        self.advance(); // for
        let var = self.expect_ident()?;
        self.expect(&TokKind::In, "em 'for x in a..b'")?;
        let from = self.parse_expr()?;
        self.expect(&TokKind::DotDot, "em 'for x in a..b'")?;
        let to = self.parse_expr()?;
        let body = self.block()?;
        Ok(StmtKind::For { var, from, to, body })
    }

    /// `func` NOME `(` `)` bloco (RFC-006, regras 1, 2 e 5).
    ///
    /// Parênteses vazios são obrigatórios: `func nome:` cai no erro
    /// genérico de `expect` (sintaxe incompleta — não precisa de mensagem
    /// especial, é erro de sintaxe comum). `func nome(x):` tem mensagem
    /// própria porque é o caso que mais confunde: parece válido, só que
    /// parâmetro não é suportado (não-objetivo 1).
    fn parse_func(&mut self) -> Result<StmtKind, ScriptError> {
        let def_line = self.line();
        if self.block_depth > 0 {
            return Err(ScriptError::new(
                def_line,
                "'func' so ecoa se gravada no nivel superior do script - tire-a de dentro do if/while/for/func",
            ));
        }
        self.advance(); // 'func'
        let name = self.expect_ident()?;
        let open_line = self.line();
        self.expect(&TokKind::LParen, "apos o nome em 'func nome()'")?;
        if self.check(&TokKind::RParen) {
            self.advance(); // ')'
        } else if self.check(&TokKind::Eof) {
            // B-005: `func combo(` nunca fechado até o EOF — mesmo padrão do
            // B-004. Reporta a linha de abertura do '(', não a do EOF, e não
            // a mensagem de "parametro não aceito" (não há parametro nenhum
            // aqui, só um delimitador que nunca fechou).
            self.expect_closing(&TokKind::RParen, open_line, "fechando os parenteses de 'func nome()'")?;
        } else {
            return Err(ScriptError::new(
                self.line(),
                "a Piramide nao aceita parametro em func: escreva 'func nome()' com os parenteses vazios",
            ));
        }
        let body = self.block()?;
        Ok(StmtKind::FuncDef { name, body })
    }

    /// `invocar` NOME bloco (RFC-004, regra 1). Diferente de `parse_func`,
    /// não há checagem de `block_depth`: `invocar` é permitido em qualquer
    /// posição de instrução, inclusive dentro de `if`/`while`/`for`/`func`
    /// (regra 2) — é ação imediata, não declaração. Reaproveita o mesmo
    /// `block()` de `parse_func`, o que basta para barrar `func` dentro de
    /// `invocar` de graça (regra 7): `block()` já incrementa
    /// `block_depth`, e é exatamente essa checagem que `parse_func` faz.
    fn parse_invoke(&mut self) -> Result<StmtKind, ScriptError> {
        self.advance(); // 'invocar'
        let name = self.expect_ident()?;
        let body = self.block()?;
        Ok(StmtKind::Invoke { name, body })
    }

    fn parse_expr(&mut self) -> Result<Expr, ScriptError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.parse_and()?;
        while self.check(&TokKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary(Box::new(left), BinOp::Or, Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.parse_cmp()?;
        while self.check(&TokKind::And) {
            self.advance();
            let right = self.parse_cmp()?;
            left = Expr::Binary(Box::new(left), BinOp::And, Box::new(right));
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek() {
                TokKind::EqEq => BinOp::Eq,
                TokKind::NotEq => BinOp::NotEq,
                TokKind::Lt => BinOp::Lt,
                TokKind::Gt => BinOp::Gt,
                TokKind::Le => BinOp::Le,
                TokKind::Ge => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_add()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::Div,
                TokKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ScriptError> {
        if self.check(&TokKind::Minus) {
            self.advance();
            let e = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryOp::Neg, Box::new(e)));
        }
        if self.check(&TokKind::Not) {
            self.advance();
            let e = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryOp::Not, Box::new(e)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ScriptError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokKind::LParen) {
                let name = match &expr {
                    Expr::Ident(name) => name.clone(),
                    _ => break,
                };
                let open_line = self.line();
                self.advance();
                let args = self.parse_args()?;
                self.expect_closing(&TokKind::RParen, open_line, "fechando chamada de funcao")?;
                expr = Expr::Call(name, args);
            } else if self.check(&TokKind::LBracket) {
                let open_line = self.line();
                self.advance();
                let idx = self.parse_expr()?;
                self.expect_closing(&TokKind::RBracket, open_line, "fechando indice")?;
                expr = Expr::Index(Box::new(expr), Box::new(idx));
            } else if self.check(&TokKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                expr = Expr::Field(Box::new(expr), field);
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ScriptError> {
        let mut args = Vec::new();
        // `Eof` aqui significa "abriu e nunca fechou" (bug B-004): deixa o
        // `expect_closing` do chamador reportar isso apontando a linha de
        // abertura do `(`, em vez de deixar `parse_expr` tentar (e falhar)
        // interpretar o fim do script como uma expressão de argumento.
        if self.check(&TokKind::RParen) || self.check(&TokKind::Eof) {
            return Ok(args);
        }
        args.push(self.parse_expr()?);
        while self.check(&TokKind::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        Ok(args)
    }

    /// `selecionar` `(` `mochila` `,` `onde` `:` <expr-bool> `,` `limite`
    /// `:` <expr> `)` (RFC-015, regra 1) — gramática fixa, não reaproveita
    /// `parse_args`: cada posição (`mochila`, `onde`, `limite`) é exigida
    /// literalmente e na ordem certa, com erro de sintaxe claro na linha
    /// quando não bate (não-objetivo 3 da RFC: `mochila` é a única fonte
    /// hoje, mas a posição já é reservada para uma futura).
    fn parse_select(&mut self) -> Result<Expr, ScriptError> {
        self.advance(); // 'selecionar'
        let open_line = self.line();
        self.expect(&TokKind::LParen, "apos 'selecionar'")?;

        let source = self.expect_ident()?;
        if source != "mochila" {
            return Err(ScriptError::new(
                self.line(),
                format!("'selecionar' so aceita 'mochila' como fonte hoje, encontrei '{source}'"),
            ));
        }
        self.expect(&TokKind::Comma, "apos 'mochila' em 'selecionar(mochila, onde: ..., limite: ...)'")?;

        let onde = self.expect_ident()?;
        if onde != "onde" {
            return Err(ScriptError::new(
                self.line(),
                format!("esperava 'onde:' em 'selecionar(mochila, onde: ..., limite: ...)', encontrei '{onde}'"),
            ));
        }
        self.expect(&TokKind::Colon, "apos 'onde' em 'selecionar(...)'")?;
        let predicate = self.parse_expr()?;

        self.expect(&TokKind::Comma, "apos a condicao de 'onde:' em 'selecionar(...)'")?;

        let limite = self.expect_ident()?;
        if limite != "limite" {
            return Err(ScriptError::new(
                self.line(),
                format!("esperava 'limite:' em 'selecionar(mochila, onde: ..., limite: ...)', encontrei '{limite}'"),
            ));
        }
        self.expect(&TokKind::Colon, "apos 'limite' em 'selecionar(...)'")?;
        let limit = self.parse_expr()?;

        self.expect_closing(&TokKind::RParen, open_line, "fechando 'selecionar(...)'")?;

        Ok(Expr::Select { predicate: Box::new(predicate), limit: Box::new(limit) })
    }

    fn parse_primary(&mut self) -> Result<Expr, ScriptError> {
        match self.peek().clone() {
            TokKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(n))
            }
            TokKind::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            TokKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokKind::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            TokKind::LParen => {
                let open_line = self.line();
                self.advance();
                let e = self.parse_expr()?;
                self.expect_closing(&TokKind::RParen, open_line, "fechando expressao entre parenteses")?;
                Ok(e)
            }
            TokKind::Selecionar => self.parse_select(),
            other => Err(ScriptError::new(self.line(), format!("token inesperado: {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zera os números de linha recursivamente — a comparação de AST entre
    /// os dois estilos de bloco deve ignorar em que linha física cada um
    /// caiu, só a *forma* da árvore importa.
    fn zero_lines(stmts: &mut [Stmt]) {
        for s in stmts.iter_mut() {
            s.line = 0;
            match &mut s.kind {
                StmtKind::If { then_branch, else_branch, .. } => {
                    zero_lines(then_branch);
                    if let Some(e) = else_branch {
                        zero_lines(e);
                    }
                }
                StmtKind::While { body, .. } => zero_lines(body),
                StmtKind::For { body, .. } => zero_lines(body),
                StmtKind::FuncDef { body, .. } => zero_lines(body),
                StmtKind::Invoke { body, .. } => zero_lines(body),
                StmtKind::Expr(_) | StmtKind::Assign(_, _) => {}
            }
        }
    }

    #[test]
    fn indent_and_brace_produce_same_ast() {
        let indent_src = "if inimigo.postura == \"guarda\":\n    defender(escudo[\"ouro\"])\nelse:\n    atacar(magia[\"fogo\"])\n";
        let brace_src = "if inimigo.postura == \"guarda\" {\n    defender(escudo[\"ouro\"])\n} else {\n    atacar(magia[\"fogo\"])\n}\n";

        let mut a = parse(indent_src).unwrap();
        let mut b = parse(brace_src).unwrap();
        zero_lines(&mut a);
        zero_lines(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn brace_block_nested_inside_indented_block_parses() {
        // indentação por fora, chaves por dentro: funciona porque a
        // indentação só é medida quando não há chaves abertas
        let src = "if inimigo.postura == \"guarda\":\n    while inimigo.vida > 0 {\n        atacar(espada[\"fogo\"])\n    }\n";
        let stmts = parse(src).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0].kind {
            StmtKind::If { then_branch, .. } => {
                assert_eq!(then_branch.len(), 1);
                assert!(matches!(then_branch[0].kind, StmtKind::While { .. }));
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn for_range_and_precedence() {
        let stmts = parse("for i in 0..3 {\n    x = 1 + 2 * 3\n}\n").unwrap();
        match &stmts[0].kind {
            StmtKind::For { var, body, .. } => {
                assert_eq!(var, "i");
                match &body[0].kind {
                    StmtKind::Assign(name, Expr::Binary(_, BinOp::Add, rhs)) => {
                        assert_eq!(name, "x");
                        assert!(matches!(**rhs, Expr::Binary(_, BinOp::Mul, _)));
                    }
                    other => panic!("unexpected stmt: {other:?}"),
                }
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn syntax_error_reports_line() {
        let err = parse("atacar(\n").unwrap_err();
        assert!(err.line >= 1);
    }

    /// Bug B-004 (achado do QA na RFC-009): parêntese aberto nunca fechado,
    /// e o jogador aperta Enter antes do EOF — o editor real
    /// (`ui/code_editor.rs::text()`) produz esse `\n` extra ao fim porque
    /// `join("\n")` não some com a linha vazia deixada pelo Enter. O erro
    /// deve apontar para a linha onde o `(` abriu (linha 2, `atacar(`), não
    /// para a linha em branco criada pelo Enter (linha 3) nem para
    /// qualquer linha "depois" dela.
    #[test]
    fn unclosed_paren_followed_by_enter_reports_the_opening_line_not_the_blank_line_after() {
        let err = parse("esperar()\natacar(\n").unwrap_err();
        assert_eq!(err.line, 2, "esperava a linha do '(' aberto, nao a linha em branco apos o Enter");
    }

    /// Mesmo caso sem o Enter extra (script termina exatamente em
    /// `atacar(`, sem newline final) — já funcionava antes da correção,
    /// e continua funcionando depois: não pode regredir.
    #[test]
    fn unclosed_paren_at_true_eof_still_reports_the_opening_line() {
        let err = parse("esperar()\natacar(").unwrap_err();
        assert_eq!(err.line, 2);
    }

    /// Generaliza o mesmo bug para um caso mais realista de dossiê do QA:
    /// o parêntese abre depois de um `if` indentado, várias linhas antes do
    /// EOF.
    #[test]
    fn unclosed_paren_inside_indented_block_reports_the_opening_line() {
        let err = parse("esperar()\nif inimigo.postura == \"guarda\":\n    atacar(\n")
            .unwrap_err();
        assert_eq!(err.line, 3);
    }

    /// A mesma classe de bug existe para `[` (índice) e `{` (bloco por
    /// chaves), já que os três delimitadores suprimem `Newline` do mesmo
    /// jeito enquanto abertos (`lexer.rs::bracket_depth`). A correção em
    /// `expect_closing` cobre os três; estes dois testes provam que não é
    /// coincidência só para `(`.
    #[test]
    fn unclosed_bracket_index_reports_the_opening_line() {
        let err = parse("esperar()\natacar(espada[\"bronze\"\n").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn unclosed_brace_block_reports_the_opening_line() {
        let err = parse("if inimigo.postura == \"guarda\" {\n    atacar(espada.Bronze)\n").unwrap_err();
        assert_eq!(err.line, 1);
    }

    /// Garante que a correção não vira martelo: quando o token inesperado
    /// não é EOF (ex.: vírgula faltando entre argumentos, ainda dentro do
    /// arquivo), a linha reportada continua sendo a do token realmente
    /// problemático, não a da abertura do parêntese.
    #[test]
    fn missing_comma_between_args_still_reports_the_offending_token_line_not_the_open_line() {
        let err = parse("atacar(\n    espada.Bronze\n    1\n)\n").unwrap_err();
        assert_eq!(err.line, 3);
    }

    #[test]
    fn func_indent_and_brace_produce_same_ast() {
        let indent_src = "func combo():\n    atacar(espada[\"ferro\"])\n\ncombo()\n";
        let brace_src = "func combo() {\n    atacar(espada[\"ferro\"])\n}\n\ncombo()\n";

        let mut a = parse(indent_src).unwrap();
        let mut b = parse(brace_src).unwrap();
        zero_lines(&mut a);
        zero_lines(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn func_def_produces_funcdef_stmt() {
        let stmts = parse("func combo():\n    atacar(espada[\"ferro\"])\n").unwrap();
        match &stmts[0].kind {
            StmtKind::FuncDef { name, body } => {
                assert_eq!(name, "combo");
                assert_eq!(body.len(), 1);
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn func_with_parameter_is_rejected_with_explicit_message() {
        let err = parse("func combo(x):\n    esperar()\n").unwrap_err();
        assert!(err.message.contains("parametro"));
    }

    /// B-005: mesmo padrao do B-004, mas em `parse_func` — o `)` que fecha a
    /// lista de parametros nunca migrou para `expect_closing`. Script exato
    /// do QA: `func combo(` nunca fechado até o EOF.
    #[test]
    fn unclosed_func_paren_reports_the_opening_line_not_eof_and_not_parameter_message() {
        let err = parse("esperar()\nfunc combo(\n").unwrap_err();
        assert_eq!(err.line, 2, "esperava a linha do '(' aberto de 'func combo(', nao a linha do EOF");
        assert!(
            !err.message.contains("parametro"),
            "nao ha parametro nenhum aqui - o parenteses so nao fechou; mensagem errada: {}",
            err.message
        );
    }

    #[test]
    fn func_without_parens_is_syntax_error() {
        let err = parse("func combo:\n    esperar()\n").unwrap_err();
        assert!(err.line >= 1);
    }

    #[test]
    fn func_nested_inside_if_is_rejected() {
        let err = parse("if true:\n    func combo():\n        esperar()\n").unwrap_err();
        assert!(err.message.contains("nivel superior"));
    }

    #[test]
    fn func_nested_inside_func_is_rejected() {
        let err = parse("func externa():\n    func interna():\n        esperar()\n").unwrap_err();
        assert!(err.message.contains("nivel superior"));
    }

    // --- RFC-015: selecionar() sobre a mochila ------------------------

    #[test]
    fn select_produces_select_expr_with_predicate_and_limit() {
        let stmts = parse("item = selecionar(mochila, onde: item.bonus > 0, limite: 1)\n").unwrap();
        match &stmts[0].kind {
            StmtKind::Assign(name, Expr::Select { predicate, limit }) => {
                assert_eq!(name, "item");
                assert!(matches!(**predicate, Expr::Binary(_, BinOp::Gt, _)));
                assert!(matches!(**limit, Expr::Number(n) if n == 1.0));
            }
            other => panic!("unexpected stmt: {other:?}"),
        }
    }

    #[test]
    fn select_source_other_than_mochila_is_a_clear_syntax_error() {
        let err = parse("selecionar(armario, onde: true, limite: 1)\n").unwrap_err();
        assert!(err.message.contains("mochila"));
    }

    #[test]
    fn select_missing_onde_label_is_a_clear_syntax_error() {
        let err = parse("selecionar(mochila, item.bonus > 0, limite: 1)\n").unwrap_err();
        assert!(err.message.contains("onde"));
    }

    #[test]
    fn select_missing_limite_label_is_a_clear_syntax_error() {
        let err = parse("selecionar(mochila, onde: true, quantidade: 1)\n").unwrap_err();
        assert!(err.message.contains("limite"));
    }
}

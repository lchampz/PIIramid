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

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
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
                format!("esperava {kind:?} em {ctx}, encontrei {:?}", self.peek()),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ScriptError> {
        match self.peek().clone() {
            TokKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(ScriptError::new(self.line(), format!("esperava um identificador, encontrei {other:?}"))),
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
    fn block(&mut self) -> Result<Vec<Stmt>, ScriptError> {
        if self.check(&TokKind::Colon) {
            self.advance();
            self.expect(&TokKind::Newline, "depois de ':'")?;
            self.expect(&TokKind::Indent, "inicio de bloco indentado")?;
            let body = self.parse_stmt_list_until(&TokKind::Dedent)?;
            self.expect(&TokKind::Dedent, "fim de bloco indentado")?;
            Ok(body)
        } else if self.check(&TokKind::LBrace) {
            self.advance();
            let body = self.parse_stmt_list_until(&TokKind::RBrace)?;
            self.expect(&TokKind::RBrace, "fim de bloco '{}'")?;
            Ok(body)
        } else {
            Err(ScriptError::new(self.line(), format!("esperava ':' ou '{{' para abrir um bloco, encontrei {:?}", self.peek())))
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ScriptError> {
        let line = self.line();
        let kind = match self.peek().clone() {
            TokKind::If => self.parse_if()?,
            TokKind::While => self.parse_while()?,
            TokKind::For => self.parse_for()?,
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
                self.advance();
                let args = self.parse_args()?;
                self.expect(&TokKind::RParen, "fechando chamada de funcao")?;
                expr = Expr::Call(name, args);
            } else if self.check(&TokKind::LBracket) {
                self.advance();
                let idx = self.parse_expr()?;
                self.expect(&TokKind::RBracket, "fechando indice")?;
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
        if self.check(&TokKind::RParen) {
            return Ok(args);
        }
        args.push(self.parse_expr()?);
        while self.check(&TokKind::Comma) {
            self.advance();
            args.push(self.parse_expr()?);
        }
        Ok(args)
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
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&TokKind::RParen, "fechando expressao entre parenteses")?;
                Ok(e)
            }
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
}

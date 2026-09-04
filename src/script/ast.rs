//! Árvore de sintaxe do pseudo-código. Deliberadamente não guarda se o
//! bloco de origem foi escrito com `:`/indentação ou com `{}` — os dois
//! estilos produzem exatamente a mesma AST (ver testes do parser).
//!
//! `Stmt` carrega a linha de origem: é o que permite à VM destacar, no
//! editor, exatamente a linha em execução durante a animação do turno.

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Str(String),
    Bool(bool),
    Ident(String),
    /// `alvo[chave]`, ex.: `magia["fogo"]`
    Index(Box<Expr>, Box<Expr>),
    /// `alvo.campo`, ex.: `inimigo.vida`
    Field(Box<Expr>, String),
    /// `nome(args...)`, ex.: `atacar(magia["fogo"])`
    Call(String, Vec<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// `selecionar(mochila, onde: <predicate>, limite: <limit>)` (RFC-015).
    /// `mochila` não é armazenada: é a única fonte suportada (não-objetivo
    /// 3 da RFC), fixa pela gramática. Sempre resolve para um único
    /// `Value::Item` ou `Value::Nil` — nunca uma coleção (não-objetivo 2).
    Select { predicate: Box<Expr>, limit: Box<Expr> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub line: usize,
    pub kind: StmtKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Expr(Expr),
    Assign(String, Expr),
    If { cond: Expr, then_branch: Vec<Stmt>, else_branch: Option<Vec<Stmt>> },
    While { cond: Expr, body: Vec<Stmt> },
    For { var: String, from: Expr, to: Expr, body: Vec<Stmt> },
    /// `func nome():` / `func nome() {` — bloco nomeado sem parâmetro
    /// (RFC-006). Declarar não executa nada; só a invocação (`Expr::Call`
    /// pelo mesmo `nome`) roda o corpo e cobra ciclo.
    FuncDef { name: String, body: Vec<Stmt> },
    /// `invocar nome:` / `invocar nome {` — bloco executado imediatamente
    /// com um sub-orçamento de ciclos próprio (RFC-004). Diferente de
    /// `FuncDef`, não é uma declaração: `name` é só rótulo narrativo/de
    /// log, nunca vira algo chamável nem acessível como variável.
    Invoke { name: String, body: Vec<Stmt> },
}

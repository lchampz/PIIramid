//! Valores em tempo de execução do pseudo-código.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Espada,
    Magia,
    Escudo,
    Pocao,
}

impl ItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Espada => "espada",
            ItemKind::Magia => "magia",
            ItemKind::Escudo => "escudo",
            ItemKind::Pocao => "pocao",
        }
    }

    pub fn from_ident(name: &str) -> Option<ItemKind> {
        match name {
            "espada" => Some(ItemKind::Espada),
            "magia" => Some(ItemKind::Magia),
            "escudo" => Some(ItemKind::Escudo),
            "pocao" => Some(ItemKind::Pocao),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Me,
    Enemy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    Str(String),
    Item(Item),
    /// valor intermediário: `espada` sozinho só é válido como base de um
    /// índice (`espada["fogo"]`), nunca usado diretamente
    Collection(ItemKind),
    /// valor intermediário: `eu`/`inimigo` só são válidos como base de um
    /// campo (`inimigo.vida`)
    EntityRef(Target),
    Nil,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Num(_) => "numero",
            Value::Bool(_) => "booleano",
            Value::Str(_) => "texto",
            Value::Item(_) => "item",
            Value::Collection(_) => "colecao",
            Value::EntityRef(_) => "entidade",
            Value::Nil => "nada",
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Nil => false,
            _ => true,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub value: Value,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: String,
    pub labels: Vec<String>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Field(Field),
    Block(Block),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub items: Vec<Item>,
}

impl Document {
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.items.iter().filter_map(|i| match i {
            Item::Field(f) => Some(f),
            Item::Block(_) => None,
        })
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match i {
            Item::Block(b) => Some(b),
            Item::Field(_) => None,
        })
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields().find(|f| f.name == name)
    }
}

impl Block {
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.items.iter().filter_map(|i| match i {
            Item::Field(f) => Some(f),
            Item::Block(_) => None,
        })
    }

    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match i {
            Item::Block(b) => Some(b),
            Item::Field(_) => None,
        })
    }

    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields().find(|f| f.name == name)
    }
}

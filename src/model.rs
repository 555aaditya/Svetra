//! The typed model of a `svetra.yaml` file, as specified in docs/format.md.

/// A source position: 1-based line, 0-based column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// A name that refers to something else, kept with its position for error reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct NameRef {
    pub name: String,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub version: String,
    pub system: NameRef,
    pub entities: Vec<Entity>,
    pub policies: Vec<Policy>,
    pub holes: Vec<Hole>,
    pub roles: Vec<RoleDef>,
}

/// A role people can hold: global (`admin`), or scoped to rows of one entity (`staff` at a `Location`).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleDef {
    pub name: String,
    pub pos: Pos,
    pub description: Option<String>,
    pub scope: Option<NameRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub name: String,
    pub pos: Pos,
    pub fields: Vec<Field>,
    pub constraints: Vec<Constraint>,
    pub unique: Vec<FieldList>,
    pub indexes: Vec<FieldList>,
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldList {
    pub fields: Vec<NameRef>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub name: String,
    pub pos: Pos,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub pos: Pos,
    pub ty: FieldType,
    pub required: bool,
    pub primary: bool,
    pub unique: bool,
    pub default: Option<Literal>,
    pub auto_now: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Text,
    Int,
    Float,
    Bool,
    Uuid,
    Timestamp,
    Date,
    Decimal { precision: u8, scale: u8 },
    Money { precision: u8, scale: u8 },
    Enum(Vec<String>),
    Json,
    Ref(NameRef),
    User,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Select,
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Signed-in people only (`to authenticated`).
    Authenticated,
    /// People who aren't signed in only (`to anon`). Rarely what you want: signed-in people get less.
    Anon,
    /// Everyone, signed in or not (`to anon, authenticated`).
    Anyone,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub name: String,
    pub pos: Pos,
    pub on: NameRef,
    pub allow: Vec<Action>,
    pub when: String,
    pub role: Role,
}

/// A hole. Version 0.1 has one kind, `function`, so the kind isn't stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub name: String,
    pub pos: Pos,
    pub input: Vec<Param>,
    pub output: Option<ParamType>,
    pub reads: Vec<NameRef>,
    pub writes: Vec<NameRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub pos: Pos,
    pub ty: ParamType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    /// `Entity.field`: the same type as that field.
    FieldOf { entity: NameRef, field: String },
    /// An entity, allowed as a hole output.
    Entity(NameRef),
    /// A bare type name or a full field spec.
    Type(FieldType),
}

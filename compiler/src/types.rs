use deref_pointer::DerefPointer;
use symbol::SymbolPointer;
use symbol_table::SymbolTable;

pub type StaticTypePointer = DerefPointer<StaticType>;

#[derive(Debug, Clone)]
pub enum Type {
    /// A dynamic type which can be freely cast to any other type.
    Dynamic,

    /// A union of 2 or more types.
    Union(Vec<Type>),

    /// A static type such as a block or object.
    Static(StaticTypePointer),
}

#[derive(Debug)]
pub enum StaticType {
    Block {
        name: Option<String>,
        locals: SymbolTable,
        arguments: Vec<SymbolPointer>,
        type_arguments: SymbolTable,
        throw_type: Option<Type>,
        return_type: Type,
    },
    Object {
        attributes: SymbolTable,
        methods: SymbolTable,
        implemented_traits: Vec<SymbolPointer>,
    },
    Class {
        name: String,
        attributes: SymbolTable,
        methods: SymbolTable,
        implemented_traits: Vec<SymbolPointer>,
        type_arguments: SymbolTable,
    },
    Trait {
        name: String,
        methods: SymbolTable,
        required_methods: SymbolTable,
        type_arguments: SymbolTable,
    },
}

/// A list of heap allocated static types.
///
/// A StaticTypeList owns the StaticType values stored in in. Dropping a
/// StaticTypeList will also drop the associated StaticType structures.
pub struct StaticTypeList {
    types: Vec<Box<StaticType>>,
}

impl StaticTypeList {
    pub fn new() -> Self {
        StaticTypeList { types: Vec::new() }
    }

    pub fn allocate(&mut self, stype: StaticType) -> StaticTypePointer {
        let boxed = Box::new(stype);
        let pointer = DerefPointer::new(&*boxed);

        self.types.push(boxed);

        pointer
    }
}

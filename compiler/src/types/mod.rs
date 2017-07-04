pub mod array;
pub mod block;
pub mod database;
pub mod float;
pub mod integer;
pub mod object;
pub mod string;
pub mod traits; // plural because "trait" is a keyword
pub mod union;

use rc_cell::RcCell;
use types::array::Array;
use types::block::Block;
use types::float::Float;
use types::integer::Integer;
use types::object::Object;
use types::string::String;
use types::traits::Trait;
use types::union::Union;

#[derive(Debug, Clone)]
pub enum Type {
    Dynamic,
    Array(RcCell<Array>),
    Block(RcCell<Block>),
    Float(RcCell<Float>),
    Integer(RcCell<Integer>),
    Object(RcCell<Object>),
    String(RcCell<String>),
    Trait(RcCell<Trait>),
    Union(RcCell<Union>),
}

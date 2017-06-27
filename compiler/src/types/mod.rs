pub mod block;
pub mod database;
pub mod integer;
pub mod object;
pub mod traits; // plural because "trait" is a keyword
pub mod union;

use rc_cell::RcCell;
use types::block::Block;
use types::integer::Integer;
use types::object::Object;
use types::traits::Trait;
use types::union::Union;

#[derive(Debug, Clone)]
pub enum Type {
    Dynamic,
    Integer(RcCell<Integer>),
    Union(RcCell<Union>),
    Block(RcCell<Block>),
    Object(RcCell<Object>),
    Trait(RcCell<Trait>),
}

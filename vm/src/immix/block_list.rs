//! Linked lists of Immix blocks.
//!
//! A BlockList is used to construct a linked list of (owned) Immix blocks. To
//! conserve space the pointer to the next block is stored in each block's
//! header.
use crate::deref_pointer::DerefPointer;
use crate::immix::block::Block;
use std::mem;
use std::ops::Index;
use std::slice::IterMut as SliceIterMut;
use std::vec::IntoIter as VecIntoIter;

/// A linked list of blocks.
#[cfg_attr(feature = "cargo-clippy", allow(vec_box))]
pub struct BlockList {
    /// The blocks managed by this BlockList. Each Block also has its "next"
    /// pointer set, allowing allocators to iterate the list while it may be
    /// modified.
    blocks: Vec<Box<Block>>,
}

/// An iterator over block pointers.
pub struct BlockIterator {
    current: DerefPointer<Block>,
}

/// An iterator over owned block pointers.
pub struct Drain {
    blocks: VecIntoIter<Box<Block>>,
}

impl BlockList {
    pub fn new() -> Self {
        BlockList { blocks: Vec::new() }
    }

    /// Pushes a block to the start of the list.
    pub fn push(&mut self, block: Box<Block>) {
        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(DerefPointer::new(&*block));
        }

        self.blocks.push(block);
    }

    /// Pops a block from the start of the list.
    pub fn pop(&mut self) -> Option<Box<Block>> {
        let block = self.blocks.pop();

        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(DerefPointer::null());
        }

        block
    }

    /// Adds the other list to the end of the current list.
    pub fn append(&mut self, other: &mut Self) {
        if let Some(last) = self.blocks.last_mut() {
            last.header_mut().set_next(other.head());
        }

        self.blocks.append(&mut other.blocks);
    }

    /// Counts the number of blocks in this list.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn head(&self) -> DerefPointer<Block> {
        if let Some(block) = self.blocks.first() {
            DerefPointer::new(&**block)
        } else {
            DerefPointer::null()
        }
    }

    /// Returns an iterator that iterates over the Vec, instead of using the
    /// "next" pointers of every block.
    pub fn iter_mut<'a>(&'a mut self) -> SliceIterMut<'a, Box<Block>> {
        self.blocks.iter_mut()
    }

    /// Returns an iterator that yields owned block pointers.
    ///
    /// Calling this method will reset the head and tail. The returned iterator
    /// will consume all blocks.
    pub fn drain(&mut self) -> Drain {
        let mut blocks = Vec::new();

        mem::swap(&mut blocks, &mut self.blocks);

        Drain {
            blocks: blocks.into_iter(),
        }
    }
}

impl Index<usize> for BlockList {
    type Output = Block;

    /// Returns a reference to the block at the given index.
    fn index(&self, index: usize) -> &Self::Output {
        &self.blocks[index]
    }
}

impl BlockIterator {
    /// Creates a new iterator starting at the given block.
    pub fn starting_at(block: &Block) -> Self {
        BlockIterator {
            current: DerefPointer::new(block),
        }
    }
}

impl Iterator for BlockIterator {
    type Item = DerefPointer<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            None
        } else {
            let current = Some(self.current);

            // One thread may be iterating (e.g. when allocating into a bucket)
            // when the other thread is adding a block. When reaching the end of
            // the list, without an atomic load we may (depending on the
            // platform) read an impartial or incorrect value.
            self.current = self.current.header().next.atomic_load();

            current
        }
    }
}

impl Iterator for Drain {
    type Item = Box<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(mut block) = self.blocks.next() {
            block.header_mut().set_next(DerefPointer::null());

            Some(block)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod block_list {
        use super::*;

        #[test]
        fn test_new() {
            let list = BlockList::new();

            assert!(list.head().is_null());
        }

        #[test]
        fn test_push_with_empty_list() {
            let block = Block::boxed();
            let mut list = BlockList::new();

            list.push(block);

            assert_eq!(list.head().is_null(), false);
        }

        #[test]
        fn test_push_with_existing_items() {
            let block1 = Block::boxed();
            let block2 = Block::boxed();
            let mut list = BlockList::new();

            list.push(block1);
            list.push(block2);

            assert_eq!(list[0].header().next.is_null(), false);
            assert!(list[1].header().next.is_null());

            assert!(
                &*list[0].header().next as *const Block
                    == &list[1] as *const Block
            );
        }

        #[test]
        fn test_pop_with_empty_list() {
            let mut list = BlockList::new();

            assert!(list.pop().is_none());
        }

        #[test]
        fn test_pop_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());
            list.push(Block::boxed());

            let block = list.pop();

            assert!(block.is_some());
            assert!(block.unwrap().header().next.is_null());
            assert!(list.blocks[0].header().next.is_null());
        }

        #[test]
        fn test_append_with_empty_lists() {
            let mut list1 = BlockList::new();
            let mut list2 = BlockList::new();

            list1.append(&mut list2);

            assert!(list1.head().is_null());
        }

        #[test]
        fn test_append_with_existing_items() {
            let mut list1 = BlockList::new();
            let mut list2 = BlockList::new();

            list1.push(Block::boxed());
            list2.push(Block::boxed());
            list1.append(&mut list2);

            assert_eq!(list1.head().is_null(), false);
            assert_eq!(list1.len(), 2);

            assert!(list2.head().is_null());
            assert_eq!(list2.len(), 0);
        }

        #[test]
        fn test_len_with_empty_list() {
            assert_eq!(BlockList::new().len(), 0);
        }

        #[test]
        fn test_len_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());

            assert_eq!(list.len(), 1);
        }

        #[test]
        fn test_is_empty_with_empty_list() {
            assert!(BlockList::new().is_empty());
        }

        #[test]
        fn test_is_empty_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());

            assert_eq!(list.is_empty(), false);
        }

        #[test]
        fn test_head_with_empty_list() {
            assert!(BlockList::new().head().is_null());
        }

        #[test]
        fn test_head_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());

            assert_eq!(list.head().is_null(), false);
        }

        #[test]
        fn test_iter_mut_with_empty_list() {
            let mut list = BlockList::new();
            let mut iter = list.iter_mut();

            assert!(iter.next().is_none());
        }

        #[test]
        fn test_iter_mut_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());

            let mut iter = list.iter_mut();

            assert!(iter.next().is_some());
            assert!(iter.next().is_none());
        }

        #[test]
        fn test_iterate_starting_at() {
            let mut list = BlockList::new();

            list.push(Block::boxed());
            list.push(Block::boxed());

            let mut iter = BlockIterator::starting_at(&list.blocks[0]);

            assert!(iter.next().is_some());
            assert!(iter.next().is_some());
            assert!(iter.next().is_none());
        }

        #[test]
        fn test_drain_with_empty_list() {
            let mut list = BlockList::new();
            let mut drain = list.drain();

            assert!(drain.next().is_none());
        }

        #[test]
        fn test_drain_with_existing_items() {
            let mut list = BlockList::new();

            list.push(Block::boxed());
            list.push(Block::boxed());

            let mut drain = list.drain();

            let block1 = drain.next().unwrap();
            let block2 = drain.next().unwrap();

            assert!(block1.header().next.is_null());
            assert!(block2.header().next.is_null());
        }
    }
}

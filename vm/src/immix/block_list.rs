//! Linked lists of Immix blocks.
//!
//! A BlockList is used to construct a linked list of (owned) Immix blocks. To
//! conserve space the pointer to the next block is stored in each block's
//! header.
use deref_pointer::DerefPointer;
use immix::block::Block;
use std::ops::Drop;
use std::ops::Index;

/// A linked list of blocks.
pub struct BlockList {
    /// The first (owned) block in the list, if any.
    pub head: Option<Box<Block>>,
}

/// An iterator over immutable block references.
pub struct BlockIterator<'a> {
    pub current: Option<&'a Block>,
}

/// An iterator over mutable block references.
pub struct BlockIteratorMut<'a> {
    pub current: Option<&'a mut Block>,
}

/// An iterator over owned block pointers.
pub struct Drain {
    pub current: Option<Box<Block>>,
}

impl BlockList {
    pub fn new() -> Self {
        BlockList { head: None }
    }

    /// Pushes a block to the start of the list.
    pub fn push_front(&mut self, mut block: Box<Block>) {
        if let Some(head) = self.head.take() {
            block.header_mut().set_next(head);
        }

        self.head = Some(block);
    }

    /// Pops a block from the start of the list.
    pub fn pop_front(&mut self) -> Option<Box<Block>> {
        self.head.take().map(|mut block| {
            self.head = block.header_mut().next.take();

            block
        })
    }

    /// Adds the other list to the end of the current list.
    pub fn append(&mut self, other: &mut Self) {
        if let Some(head) = other.head.take() {
            if let Some(last) = self.iter_mut().last() {
                last.header_mut().set_next(head);
                return;
            }

            self.head = Some(head);
        }
    }

    /// Counts the number of blocks in this list.
    ///
    /// This method will traverse all blocks and as such should not be called in
    /// a tight loop.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Returns a mutable reference to the head of this list.
    pub fn head_mut(&mut self) -> Option<&mut Block> {
        self.head.as_mut().map(|block| &mut **block)
    }

    /// Returns an immutable reference to the head of this list.
    pub fn head(&self) -> Option<&Block> {
        self.head.as_ref().map(|block| &**block)
    }

    pub fn iter(&self) -> BlockIterator {
        BlockIterator {
            current: self.head.as_ref().map(|block| &**block),
        }
    }

    pub fn iter_mut(&mut self) -> BlockIteratorMut {
        BlockIteratorMut {
            current: self.head.as_mut().map(|block| &mut **block),
        }
    }

    /// Returns an iterator that yields owned block pointers.
    ///
    /// Calling this method will reset the head and tail. The returned iterator
    /// will consume all blocks.
    pub fn drain(&mut self) -> Drain {
        Drain {
            current: self.head.take(),
        }
    }

    /// Returns a vector containing pointers to the blocks in this list.
    pub fn pointers(&self) -> Vec<DerefPointer<Block>> {
        self.iter().map(|block| DerefPointer::new(block)).collect()
    }
}

impl Index<usize> for BlockList {
    type Output = Block;

    /// Returns a reference to the block at the given index.
    ///
    /// This method will iterate over all blocks in this list, making it quite
    /// slow.
    fn index(&self, given_index: usize) -> &Self::Output {
        for (index, block) in self.iter().enumerate() {
            if index == given_index {
                return block;
            }
        }

        panic!("can not use out-of-bounds index {}", given_index);
    }
}

impl<'a> BlockIteratorMut<'a> {
    /// Creates a new mutable iterator starting at the given block.
    pub fn starting_at(block: &'a mut Block) -> Self {
        BlockIteratorMut {
            current: Some(block),
        }
    }
}

impl<'a> Iterator for BlockIterator<'a> {
    type Item = &'a Block;

    fn next(&mut self) -> Option<Self::Item> {
        self.current.map(|block| {
            self.current = block.header().next.as_ref().map(|next| &**next);
            block
        })
    }
}

impl<'a> Iterator for BlockIteratorMut<'a> {
    type Item = &'a mut Block;

    fn next(&mut self) -> Option<Self::Item> {
        self.current.take().map(|block| {
            // Rust doesn't like that we need to grab a mutable reference to the
            // header of this block in order to store the next element. To work
            // around this we turn "block" into a pointer, then turn this into a
            // separate mutable reference.
            let for_next = unsafe { &mut *(block as *mut Block) };

            self.current = for_next
                .header_mut()
                .next
                .as_mut()
                .map(|block| &mut **block);

            block
        })
    }
}

impl Iterator for Drain {
    type Item = Box<Block>;

    fn next(&mut self) -> Option<Self::Item> {
        self.current.take().map(|mut block| {
            self.current = block.header_mut().next.take();
            block
        })
    }
}

impl Drop for BlockList {
    fn drop(&mut self) {
        // We need to explicitly traverse through all blocks as otherwise Rust
        // won't drop them due to the "next" pointers being stored in block
        // headers.
        for block in self.drain() {
            drop(block);
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

            assert!(list.head.is_none());
        }

        #[test]
        fn test_push_front_with_empty_list() {
            let block = Block::boxed();
            let mut list = BlockList::new();

            list.push_front(block);

            assert!(list.head.is_some());
        }

        #[test]
        fn test_push_front_with_existing_items() {
            let block1 = Block::boxed();
            let block2 = Block::boxed();
            let mut list = BlockList::new();

            list.push_front(block1);
            list.push_front(block2);

            assert!(list[0].header().next.is_some());
            assert!(list[1].header().next.is_none());

            assert!(
                &**list[0].header().next.as_ref().unwrap() as *const Block
                    == &list[1] as *const Block
            );
        }

        #[test]
        fn test_pop_front_with_empty_list() {
            let mut list = BlockList::new();

            assert!(list.pop_front().is_none());
        }

        #[test]
        fn test_pop_front_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            let block = list.pop_front();

            assert!(block.is_some());
            assert!(block.unwrap().header().next.is_none());
        }

        #[test]
        fn test_append_with_empty_lists() {
            let mut list1 = BlockList::new();
            let mut list2 = BlockList::new();

            list1.append(&mut list2);

            assert!(list1.head.is_none());
        }

        #[test]
        fn test_append_with_existing_items() {
            let mut list1 = BlockList::new();
            let mut list2 = BlockList::new();

            list1.push_front(Block::boxed());
            list2.push_front(Block::boxed());
            list1.append(&mut list2);

            assert!(list1.head.is_some());
            assert_eq!(list1.len(), 2);

            assert!(list2.head.is_none());
            assert_eq!(list2.len(), 0);
        }

        #[test]
        fn test_len_with_empty_list() {
            assert_eq!(BlockList::new().len(), 0);
        }

        #[test]
        fn test_len_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            assert_eq!(list.len(), 1);
        }

        #[test]
        fn test_is_empty_with_empty_list() {
            assert!(BlockList::new().is_empty());
        }

        #[test]
        fn test_is_empty_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            assert_eq!(list.is_empty(), false);
        }

        #[test]
        fn test_head_mut_with_empty_list() {
            assert!(BlockList::new().head_mut().is_none());
        }

        #[test]
        fn test_head_mut_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            assert!(list.head_mut().is_some());
        }

        #[test]
        fn test_head_with_empty_list() {
            assert!(BlockList::new().head().is_none());
        }

        #[test]
        fn test_head_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            assert!(list.head().is_some());
        }

        #[test]
        fn test_iter_with_empty_list() {
            let list = BlockList::new();
            let mut iter = list.iter();

            assert!(iter.next().is_none());
        }

        #[test]
        fn test_iter_with_existing_items() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            let mut iter = list.iter();

            assert!(iter.next().is_some());
            assert!(iter.next().is_none());
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

            list.push_front(Block::boxed());

            let mut iter = list.iter_mut();

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

            list.push_front(Block::boxed());
            list.push_front(Block::boxed());

            let mut drain = list.drain();

            let block1 = drain.next().unwrap();
            let block2 = drain.next().unwrap();

            assert!(block1.header().next.is_none());
            assert!(block2.header().next.is_none());
        }

        #[test]
        fn test_pointers() {
            let mut list = BlockList::new();

            list.push_front(Block::boxed());

            assert_eq!(list.pointers().len(), 1);
        }
    }
}

#![macro_use]

/// Macro for allocating objects into a bucket.
macro_rules! allocate_in_bucket {
    ($alloc: expr, $object: ident, $bucket: expr) => ({
        let has_hole = $bucket.find_hole();

        if !has_hole {
            let block = $alloc.global_allocator.request_block();

            $bucket.add_block(block);
        }

        (!has_hole, $bucket.bump_allocate($object))
    });
}

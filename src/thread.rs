use heap;

pub struct Thread {
    young_heap: heap::Heap,
    mature_heap: heap::Heap
}

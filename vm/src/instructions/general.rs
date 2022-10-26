//! VM functions for which no better category/module exists.
use crate::execution_context::ExecutionContext;
use crate::indexes::{ClassIndex, FieldIndex, MethodIndex};
use crate::mem::{Class, Header, Int, Object, Pointer};
use crate::process::TaskPointer;
use crate::state::State;
use bytecode::{REF_ATOMIC, REF_OWNED, REF_PERMANENT, REF_REF};

#[inline(always)]
pub(crate) fn allocate(state: &State, idx: u32) -> Pointer {
    let index = ClassIndex::new(idx);
    let class = unsafe { state.permanent_space.get_class(index) };

    Object::alloc(class)
}

#[inline(always)]
pub(crate) fn get_field(receiver: Pointer, index: u16) -> Pointer {
    unsafe { receiver.get::<Object>().get_field(FieldIndex::new(index as u8)) }
}

#[inline(always)]
pub(crate) fn set_field(receiver: Pointer, index: u16, value: Pointer) {
    unsafe {
        receiver
            .get_mut::<Object>()
            .set_field(FieldIndex::new(index as u8), value);
    }
}

#[inline(always)]
pub(crate) fn equals(compare: Pointer, compare_with: Pointer) -> Pointer {
    if compare == compare_with {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn call_virtual(
    state: &State,
    mut task: TaskPointer,
    receiver: Pointer,
    method_idx: u16,
) {
    let class = Class::of(&state.permanent_space, receiver);
    let method = unsafe { class.get_method(MethodIndex::new(method_idx)) };

    task.push_context(ExecutionContext::new(method));
}

#[inline(always)]
pub(crate) fn call_static(
    state: &State,
    mut task: TaskPointer,
    class_idx: u32,
    method_idx: u16,
) {
    let class =
        unsafe { state.permanent_space.get_class(ClassIndex::new(class_idx)) };
    let method = unsafe { class.get_method(MethodIndex::new(method_idx)) };

    task.push_context(ExecutionContext::new(method));
}

#[inline(always)]
pub(crate) fn call_dynamic(
    state: &State,
    mut task: TaskPointer,
    receiver: Pointer,
    hash: u32,
) {
    let class = Class::of(&state.permanent_space, receiver);
    let method = unsafe { class.get_hashed_method(hash) };

    task.push_context(ExecutionContext::new(method));
}

#[inline(always)]
pub(crate) fn exit(state: &State, status_ptr: Pointer) -> Result<(), String> {
    let status = unsafe { Int::read(status_ptr) as i32 };

    state.set_exit_status(status);
    state.terminate();
    Ok(())
}

#[inline(always)]
pub(crate) fn increment(pointer: Pointer) -> Pointer {
    if !pointer.is_local_heap_object() {
        return pointer;
    }

    let header = unsafe { pointer.get_mut::<Header>() };

    if header.is_atomic() {
        header.increment_atomic();
        pointer
    } else {
        header.increment();
        pointer.as_ref()
    }
}

#[inline(always)]
pub(crate) fn decrement(pointer: Pointer) {
    if !pointer.is_local_heap_object() {
        return;
    }

    unsafe { pointer.get_mut::<Header>() }.decrement();
}

#[inline(always)]
pub(crate) fn decrement_atomic(pointer: Pointer) -> Pointer {
    if !pointer.is_local_heap_object() {
        return Pointer::false_singleton();
    }

    let header = unsafe { pointer.get_mut::<Header>() };

    if header.decrement_atomic() {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn check_refs(pointer: Pointer) -> Result<(), String> {
    if !pointer.is_local_heap_object() {
        return Ok(());
    }

    let header = unsafe { pointer.get::<Header>() };
    let refs = header.references();

    if refs == 0 {
        return Ok(());
    }

    Err(format!(
        "Can't drop a value of type '{}' as it still has {} references",
        &header.class.name, refs
    ))
}

#[inline(always)]
pub(crate) fn ref_kind(pointer: Pointer) -> Pointer {
    if !pointer.is_local_heap_object() {
        Pointer::int(REF_PERMANENT as i64)
    } else if unsafe { pointer.get_mut::<Header>().is_atomic() } {
        Pointer::int(REF_ATOMIC as i64)
    } else if pointer.is_ref() {
        Pointer::int(REF_REF as i64)
    } else {
        Pointer::int(REF_OWNED as i64)
    }
}

#[inline(always)]
pub(crate) fn free(ptr: Pointer) {
    if ptr.is_local_heap_object() {
        unsafe {
            ptr.free();
        }
    }
}

#[inline(always)]
pub(crate) fn is_undefined(pointer: Pointer) -> Pointer {
    if pointer == Pointer::undefined_singleton() {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

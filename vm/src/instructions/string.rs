//! VM functions for working with Inko strings.
use crate::mem::Pointer;
use crate::mem::{Int, String as InkoString};
use crate::process::TaskPointer;
use crate::state::State;

#[inline(always)]
pub(crate) fn equals(left_ptr: Pointer, right_ptr: Pointer) -> Pointer {
    if left_ptr.is_permanent() && right_ptr.is_permanent() {
        if left_ptr == right_ptr {
            return Pointer::true_singleton();
        } else {
            return Pointer::false_singleton();
        }
    }

    let left = unsafe { InkoString::read(&left_ptr) };
    let right = unsafe { InkoString::read(&right_ptr) };

    if left == right {
        Pointer::true_singleton()
    } else {
        Pointer::false_singleton()
    }
}

#[inline(always)]
pub(crate) fn size(state: &State, ptr: Pointer) -> Pointer {
    let string = unsafe { InkoString::read(&ptr) };
    let length = string.len() as i64;

    Int::alloc(state.permanent_space.int_class(), length)
}

#[inline(always)]
pub(crate) fn concat(state: &State, mut task: TaskPointer) -> Pointer {
    let mut buffer = String::new();

    for ptr in task.take_arguments() {
        buffer.push_str(unsafe { InkoString::read(&ptr) });
    }

    InkoString::alloc(state.permanent_space.string_class(), buffer)
}

#[inline(always)]
pub(crate) fn byte(str_ptr: Pointer, index_ptr: Pointer) -> Pointer {
    let byte = unsafe {
        let string = InkoString::read(&str_ptr);
        let index = Int::read(index_ptr) as usize;

        i64::from(*string.as_bytes().get_unchecked(index))
    };

    Pointer::int(byte)
}

#[inline(always)]
pub(crate) fn drop(pointer: Pointer) {
    // Permanent Strings can be used as if they were regular owned Strings, so
    // we must make sure not to drop these.
    if pointer.is_permanent() {
        return;
    }

    unsafe {
        InkoString::drop(pointer);
    }
}

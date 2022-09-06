//! VM functions for working with Inko modules.
use crate::indexes::ModuleIndex;
use crate::mem::Pointer;
use crate::state::State;

#[inline(always)]
pub(crate) fn get(state: &State, idx: u32) -> Pointer {
    let index = ModuleIndex::new(idx);

    unsafe { state.permanent_space.get_module(index).as_pointer() }
}

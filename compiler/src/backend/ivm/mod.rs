use state::State;
use tir::module::Module;

pub struct Ivm<'a> {
    state: &'a mut State,
}

impl<'a> Ivm<'a> {
    pub fn new(state: &'a mut State) -> Self {
        Ivm { state: state }
    }

    pub fn compile(&mut self, module: Module) {
        println!("{:#?}", module);
    }
}

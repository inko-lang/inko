use crate::codegen::layouts::Layouts;
use crate::config::{BuildDirectories, Opt};
use crate::mir::Mir;
use crate::state::State;
use crate::symbol_names::SymbolNames;
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::ObjectModule;

/// A pass that lowers MIR modules into Cranelift IR modules.
pub(crate) struct Lower {
    // TODO
}

impl<'a> Lower {
    pub(crate) fn run_all(
        state: &'a State,
        directories: &BuildDirectories,
        mir: &'a Mir,
    ) -> Vec<ObjectModule> {
        let mut conf = settings::builder();
        let opt_level = match state.config.opt {
            Opt::None => "none",
            Opt::Balanced => "speed", // TODO: benchmark
            Opt::Aggressive => "speed_and_size",
        };

        conf.set("opt_level", opt_level).unwrap();
        conf.enable("is_pic").unwrap();

        let flags = settings::Flags::new(conf);
        let target = isa::lookup_by_name(&state.config.target.llvm_triple())
            .unwrap()
            .finish(flags)
            .unwrap();

        let types = Layouts::new(
            state,
            mir,
            target.triple().pointer_width().unwrap() as u16,
        );

        let names = SymbolNames::new(&state.db, mir);
        let mut modules = Vec::with_capacity(mir.modules.len());

        for module_index in 0..mir.modules.len() {
            let mod_id = mir.modules[module_index].id;
            let name = mod_id.name(&state.db).clone();
            let path = mod_id.file(&state.db);
        }

        modules
    }
}

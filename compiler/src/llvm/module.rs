use crate::llvm::builder::DebugBuilder;
use crate::llvm::context::Context;
use crate::llvm::layouts::Layouts;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::mir::Constant;
use crate::symbol_names::SYMBOL_PREFIX;
use inkwell::intrinsics::Intrinsic;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, FunctionValue, GlobalValue};
use inkwell::{module, AddressSpace};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use types::module_name::ModuleName;
use types::{ClassId, MethodId};

/// A wrapper around an LLVM Module that provides some additional methods.
pub(crate) struct Module<'a, 'ctx> {
    pub(crate) inner: module::Module<'ctx>,
    pub(crate) context: &'ctx Context,
    pub(crate) name: ModuleName,
    pub(crate) layouts: &'a Layouts<'ctx>,
    pub(crate) literals: HashMap<Constant, GlobalValue<'ctx>>,
    pub(crate) debug_builder: DebugBuilder<'ctx>,
}

impl<'a, 'ctx> Module<'a, 'ctx> {
    pub(crate) fn new(
        context: &'ctx Context,
        layouts: &'a Layouts<'ctx>,
        name: ModuleName,
        path: &Path,
    ) -> Self {
        let inner = context.create_module(name.as_str());
        let debug_builder = DebugBuilder::new(&inner, context, path);

        Self {
            inner,
            context,
            name,
            layouts,
            literals: HashMap::new(),
            debug_builder,
        }
    }

    pub(crate) fn add_global(&self, name: &str) -> GlobalValue<'ctx> {
        let typ = self.context.pointer_type();
        let space = AddressSpace::default();

        self.inner.add_global(typ, Some(space), name)
    }

    pub(crate) fn add_literal(
        &mut self,
        value: &Constant,
    ) -> GlobalValue<'ctx> {
        if let Some(&global) = self.literals.get(value) {
            global
        } else {
            let name = format!(
                "{}L_{}_{}",
                SYMBOL_PREFIX,
                self.name,
                self.literals.len()
            );

            let global = self.add_global(&name);

            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );

            self.literals.insert(value.clone(), global);
            global
        }
    }

    pub(crate) fn add_constant(&mut self, name: &str) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| self.add_global(name))
    }

    pub(crate) fn add_class(
        &mut self,
        id: ClassId,
        name: &str,
    ) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| {
            let space = AddressSpace::default();
            let typ = self.layouts.classes[&id].ptr_type(space);

            self.inner.add_global(typ, Some(space), name)
        })
    }

    pub(crate) fn add_method(
        &self,
        name: &str,
        method: MethodId,
    ) -> FunctionValue<'ctx> {
        self.inner.get_function(name).unwrap_or_else(|| {
            self.inner.add_function(
                name,
                self.layouts.methods[&method].signature,
                None,
            )
        })
    }

    pub(crate) fn add_setup_function(&self, name: &str) -> FunctionValue<'ctx> {
        if let Some(func) = self.inner.get_function(name) {
            func
        } else {
            let space = AddressSpace::default();
            let args = [self.layouts.state.ptr_type(space).into()];
            let typ = self.context.void_type().fn_type(&args, false);

            self.inner.add_function(name, typ, None)
        }
    }

    pub(crate) fn runtime_function(
        &self,
        function: RuntimeFunction,
    ) -> FunctionValue<'ctx> {
        self.inner
            .get_function(function.name())
            .unwrap_or_else(|| function.build(self))
    }

    pub(crate) fn intrinsic(
        &self,
        name: &str,
        args: &[BasicTypeEnum<'ctx>],
    ) -> FunctionValue<'ctx> {
        Intrinsic::find(name)
            .and_then(|intr| intr.get_declaration(&self.inner, args))
            .unwrap()
    }
}

impl<'a, 'ctx> Deref for Module<'a, 'ctx> {
    type Target = module::Module<'ctx>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

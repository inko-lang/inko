use crate::llvm::builder::DebugBuilder;
use crate::llvm::context::Context;
use crate::llvm::layouts::Layouts;
use crate::llvm::runtime_function::RuntimeFunction;
use crate::symbol_names::SYMBOL_PREFIX;
use inkwell::attributes::AttributeLoc;
use inkwell::intrinsics::Intrinsic;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, FunctionValue, GlobalValue};
use inkwell::{module, AddressSpace};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use types::module_name::ModuleName;
use types::{CallConvention, MethodId};

/// A wrapper around an LLVM Module that provides some additional methods.
pub(crate) struct Module<'a, 'ctx> {
    pub(crate) inner: module::Module<'ctx>,
    pub(crate) context: &'ctx Context,
    pub(crate) name: ModuleName,
    pub(crate) layouts: &'a Layouts<'ctx>,
    pub(crate) strings: HashMap<String, GlobalValue<'ctx>>,
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
            strings: HashMap::new(),
            debug_builder,
        }
    }

    pub(crate) fn add_global_pointer(&self, name: &str) -> GlobalValue<'ctx> {
        self.add_global(self.context.pointer_type(), name)
    }

    pub(crate) fn add_global<T: BasicType<'ctx>>(
        &self,
        typ: T,
        name: &str,
    ) -> GlobalValue<'ctx> {
        self.inner.add_global(typ, Some(AddressSpace::default()), name)
    }

    pub(crate) fn add_string(&mut self, value: &String) -> GlobalValue<'ctx> {
        if let Some(&global) = self.strings.get(value) {
            global
        } else {
            let name = format!(
                "{}S_{}_{}",
                SYMBOL_PREFIX,
                self.name,
                self.strings.len()
            );

            let global = self.add_global_pointer(&name);

            global.set_initializer(
                &self.context.pointer_type().const_null().as_basic_value_enum(),
            );

            self.strings.insert(value.clone(), global);
            global
        }
    }

    pub(crate) fn add_constant(&mut self, name: &str) -> GlobalValue<'ctx> {
        self.inner
            .get_global(name)
            .unwrap_or_else(|| self.add_global_pointer(name))
    }

    pub(crate) fn add_class(&mut self, name: &str) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| {
            let typ = self.context.pointer_type();
            let space = typ.get_address_space();

            self.inner.add_global(typ, Some(space), name)
        })
    }

    pub(crate) fn add_method(
        &self,
        name: &str,
        method: MethodId,
    ) -> FunctionValue<'ctx> {
        self.inner.get_function(name).unwrap_or_else(|| {
            let info = &self.layouts.methods[method.0 as usize];
            let func = self.inner.add_function(name, info.signature, None);
            let conv = match info.call_convention {
                // LLVM uses 0 for the C calling convention.
                CallConvention::C => 0,

                // For the time being the Inko calling convention is the same as
                // the C calling convention, but this may change in the future.
                CallConvention::Inko => 0,
            };

            func.set_call_conventions(conv);

            if let Some(typ) = info.struct_return {
                let sret = self.context.type_attribute("sret", typ.into());
                let noalias = self.context.enum_attribute("noalias", 0);
                let nocapt = self.context.enum_attribute("nocapture", 0);

                func.add_attribute(AttributeLoc::Param(0), sret);
                func.add_attribute(AttributeLoc::Param(0), noalias);
                func.add_attribute(AttributeLoc::Param(0), nocapt);
            }

            func
        })
    }

    pub(crate) fn add_setup_function(&self, name: &str) -> FunctionValue<'ctx> {
        if let Some(func) = self.inner.get_function(name) {
            func
        } else {
            let typ = self.context.void_type().fn_type(&[], false);

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

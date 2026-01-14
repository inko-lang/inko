use crate::llvm::builder::DebugBuilder;
use crate::llvm::context::Context;
use crate::llvm::layouts::{ArgumentType, Layouts};
use crate::llvm::runtime_function::RuntimeFunction;
use crate::symbol_names::SYMBOL_PREFIX;
use inkwell::attributes::AttributeLoc;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Linkage;
use inkwell::types::{BasicType, BasicTypeEnum, StructType};
use inkwell::values::{BasicValue, FunctionValue, GlobalValue};
use inkwell::{module, AddressSpace};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::Path;
use types::module_name::ModuleName;
use types::{Block, CallConvention, Database, MethodId};

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

    pub(crate) fn add_static_global<T: BasicType<'ctx>, V: BasicValue<'ctx>>(
        &self,
        typ: T,
        value: V,
    ) -> GlobalValue<'ctx> {
        let glob =
            self.inner.add_global(typ, Some(AddressSpace::default()), "");

        glob.set_linkage(Linkage::Private);
        glob.set_initializer(&value);
        glob
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

    pub(crate) fn add_constant<T: BasicType<'ctx>>(
        &mut self,
        name: &str,
        typ: T,
    ) -> GlobalValue<'ctx> {
        self.inner
            .get_global(name)
            .unwrap_or_else(|| self.add_global(typ, name))
    }

    pub(crate) fn add_type(
        &self,
        name: &str,
        typ: StructType<'ctx>,
    ) -> GlobalValue<'ctx> {
        self.inner.get_global(name).unwrap_or_else(|| {
            self.inner.add_global(typ, Some(AddressSpace::default()), name)
        })
    }

    pub(crate) fn add_method(
        &self,
        db: &Database,
        name: &str,
        method: MethodId,
    ) -> FunctionValue<'ctx> {
        self.inner.get_function(name).unwrap_or_else(|| {
            let info = &self.layouts.methods[method.0 as usize];
            let fn_typ = info.signature(self.context);
            let fn_val = self.inner.add_function(name, fn_typ, None);
            let conv = match info.call_convention {
                // LLVM uses 0 for the C calling convention.
                CallConvention::C => 0,

                // For the time being the Inko calling convention is the same as
                // the C calling convention, but this may change in the future.
                CallConvention::Inko => 0,
            };

            fn_val.set_call_conventions(conv);

            let mut sret = false;

            for (idx, &arg) in info.arguments.iter().enumerate() {
                match arg {
                    ArgumentType::StructValue(t) => {
                        fn_val.add_attribute(
                            AttributeLoc::Param(idx as _),
                            self.context.type_attribute("byval", t.into()),
                        );
                    }
                    ArgumentType::StructReturn(t) => {
                        // For struct returns we use a hidden first argument, so
                        // we need to shift the first user-defined argument to
                        // the right.
                        sret = true;

                        let loc = AttributeLoc::Param(0);
                        let sret =
                            self.context.type_attribute("sret", t.into());
                        let noalias = self.context.flag("noalias");
                        let nocapt = self.context.no_capture_flag();

                        fn_val.add_attribute(loc, sret);
                        fn_val.add_attribute(loc, noalias);
                        fn_val.add_attribute(loc, nocapt);
                    }
                    _ => {}
                }
            }

            // Add various attributes to the function arguments, in order to
            // (hopefully) produce better optimized code.
            if method.is_async(db) {
                // async methods use the signature `fn(message)` where the
                // message is unpacked into the individual arguments.
                let loc = AttributeLoc::Param(0);
                let ro = self.context.flag("readonly");
                let noal = self.context.flag("noalias");
                let nocap = self.context.no_capture_flag();

                fn_val.add_attribute(loc, ro);
                fn_val.add_attribute(loc, noal);
                fn_val.add_attribute(loc, nocap);
            } else {
                let llvm_args = fn_typ.get_param_types();
                let is_instance = method.is_instance(db);
                let first_arg =
                    if is_instance { 1 } else { 0 } + (sret as usize);

                if is_instance {
                    let idx = if sret { 1_usize } else { 0 };
                    let loc = AttributeLoc::Param(idx as u32);

                    if llvm_args[idx].is_pointer_type() {
                        fn_val.add_attribute(loc, self.context.flag("nonnull"));
                    }

                    fn_val.add_attribute(loc, self.context.flag("noundef"));
                }

                for (idx, &typ) in method.argument_types(db).enumerate() {
                    let idx = idx + first_arg;
                    let loc = AttributeLoc::Param(idx as _);
                    let ltyp = llvm_args[idx];

                    // For borrows we _don't_ set the "readonly" attribute, as
                    // we modify the reference count through the pointer.
                    if ltyp.is_pointer_type() {
                        if typ.is_uni_value(db) {
                            let attr = self.context.flag("noalias");

                            fn_val.add_attribute(loc, attr);
                        } else if typ.is_ref_or_mut(db) {
                            // We never release memory through borrows.
                            let attr = self.context.flag("nofree");

                            fn_val.add_attribute(loc, attr);
                        }

                        // Inko values passed as a pointer (e.g. a borrow) are
                        // never NULL.
                        if !typ.is_foreign_type(db) {
                            let not_null = self.context.flag("nonnull");
                            let not_undef = self.context.flag("noundef");

                            fn_val.add_attribute(loc, not_null);
                            fn_val.add_attribute(loc, not_undef);
                        }
                    }

                    // Inko's own types are always initialized.
                    if !typ.is_foreign_type(db) {
                        let attr = self.context.flag("noundef");

                        fn_val.add_attribute(loc, attr);
                    }
                }
            }

            if method.return_type(db).is_never(db) {
                let cold = self.context.flag("cold");
                let noin = self.context.flag("noinline");
                let noret = self.context.flag("noreturn");

                fn_val.add_attribute(AttributeLoc::Function, cold);
                fn_val.add_attribute(AttributeLoc::Function, noin);
                fn_val.add_attribute(AttributeLoc::Function, noret);
            }

            fn_val
        })
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

use crate::config::BuildDirectories;
use crate::hir;
use crate::json::{Json, Object};
use crate::state::State;
use location::Location;
use std::fs::{read_to_string, write};
use std::mem::take;
use std::path::Path;
use types::format::{format_type, type_parameter_capabilities};
use types::{
    Database, MethodId, ModuleId, TraitId, TypeBounds, TypeId, TypeKind,
};

fn location_to_json(location: Location) -> Json {
    let mut obj = Object::new();
    let mut lines = Object::new();
    let mut cols = Object::new();

    lines.add("start", Json::Int(location.line_start as i64));
    lines.add("end", Json::Int(location.line_end as i64));
    cols.add("start", Json::Int(location.column_start as i64));
    cols.add("end", Json::Int(location.column_end as i64));
    obj.add("lines", Json::Object(lines));
    obj.add("columns", Json::Object(cols));
    Json::Object(obj)
}

fn type_kind(kind: TypeKind, copy: bool) -> i64 {
    if copy {
        return 4;
    }

    match kind {
        TypeKind::Enum => 1,
        TypeKind::Async => 2,
        TypeKind::Extern => 3,
        TypeKind::Atomic => 5,
        _ => 0,
    }
}

fn format_bounds(db: &Database, bounds: &TypeBounds) -> String {
    let mut buf = String::new();
    let mut pairs =
        bounds.iter().map(|(k, v)| (k.name(db), v)).collect::<Vec<_>>();

    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
    buf.push_str("\nif\n");

    for (idx, (param, &req)) in pairs.into_iter().enumerate() {
        let reqs = req.requirements(db);
        let capa = type_parameter_capabilities(db, req);

        buf.push_str(&format!(
            "{}  {}: {}",
            if idx > 0 { ",\n" } else { "" },
            param,
            capa.unwrap_or("")
        ));

        if !reqs.is_empty() {
            if capa.is_some() {
                buf.push_str(" + ");
            }

            buf.push_str(
                &reqs
                    .into_iter()
                    .map(|v| format_type(db, v))
                    .collect::<Vec<_>>()
                    .join(" + "),
            );
        }
    }

    buf
}

fn format_method(db: &Database, id: MethodId) -> String {
    let typ = format_type(db, id);
    let bounds = id.bounds(db);

    if bounds.is_empty() {
        typ
    } else {
        // For documentation purposes we include the bounds, which isn't
        // included in the type signatures produced for compiler diagnostics.
        typ + &format_bounds(db, bounds)
    }
}

/// A type used to configure the documentation generation process.
pub struct Config {
    pub private: bool,
    pub dependencies: bool,
}

/// A compiler pass that defines the documentation of symbols based on the
/// source comments.
pub(crate) struct DefineDocumentation<'a> {
    state: &'a mut State,
    module: ModuleId,
}

impl<'a> DefineDocumentation<'a> {
    pub(crate) fn run_all(
        state: &'a mut State,
        modules: &mut Vec<hir::Module>,
    ) {
        for module in modules {
            DefineDocumentation { state, module: module.module_id }.run(module);
        }
    }

    fn run(mut self, module: &mut hir::Module) {
        self.module
            .set_documentation(self.db_mut(), take(&mut module.documentation));

        for expr in &mut module.expressions {
            match expr {
                hir::TopLevelExpression::Type(n) => {
                    n.type_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );

                    self.define_type(&mut *n);
                }
                hir::TopLevelExpression::ExternType(n) => {
                    n.type_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );

                    self.define_extern_type(&mut *n);
                }
                hir::TopLevelExpression::Constant(n) => {
                    n.constant_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TopLevelExpression::ModuleMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TopLevelExpression::ExternFunction(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TopLevelExpression::Trait(n) => {
                    n.trait_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );

                    self.define_trait(&mut *n);
                }
                hir::TopLevelExpression::Implement(n) => {
                    self.implement_trait(&mut *n);
                }
                hir::TopLevelExpression::Reopen(n) => {
                    self.reopen_type(&mut *n);
                }
                _ => {}
            }
        }
    }

    fn define_type(&mut self, node: &mut hir::DefineType) {
        for expr in &mut node.body {
            match expr {
                hir::TypeExpression::InstanceMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TypeExpression::StaticMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TypeExpression::AsyncMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TypeExpression::Field(n) => {
                    n.field_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TypeExpression::Constructor(n) => {
                    n.constructor_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
            }
        }
    }

    fn define_extern_type(&mut self, node: &mut hir::DefineExternType) {
        for n in &mut node.fields {
            n.field_id
                .unwrap()
                .set_documentation(self.db_mut(), take(&mut n.documentation));
        }
    }

    fn define_trait(&mut self, node: &mut hir::DefineTrait) {
        for expr in &mut node.body {
            match expr {
                hir::TraitExpression::InstanceMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::TraitExpression::RequiredMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
            }
        }
    }

    fn implement_trait(&mut self, node: &mut hir::ImplementTrait) {
        for n in &mut node.body {
            n.method_id
                .unwrap()
                .set_documentation(self.db_mut(), take(&mut n.documentation));
        }
    }

    fn reopen_type(&mut self, node: &mut hir::ReopenType) {
        for expr in &mut node.body {
            match expr {
                hir::ReopenTypeExpression::InstanceMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::ReopenTypeExpression::StaticMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
                hir::ReopenTypeExpression::AsyncMethod(n) => {
                    n.method_id.unwrap().set_documentation(
                        self.db_mut(),
                        take(&mut n.documentation),
                    );
                }
            }
        }
    }

    fn db_mut(&mut self) -> &mut Database {
        &mut self.state.db
    }
}

pub(crate) struct GenerateDocumentation<'a> {
    state: &'a State,
    directory: &'a Path,
    module: ModuleId,
    config: &'a Config,
}

impl<'a> GenerateDocumentation<'a> {
    pub(crate) fn run_all(
        state: &'a State,
        directories: &BuildDirectories,
        config: &'a Config,
    ) -> Result<(), String> {
        for idx in 0..state.db.number_of_modules() {
            let id = ModuleId(idx as _);
            let file = id.file(&state.db);

            if state.config.source != state.config.std
                && !config.dependencies
                && (file.starts_with(&state.config.dependencies)
                    || file.starts_with(&state.config.std))
            {
                continue;
            }

            GenerateDocumentation {
                state,
                directory: &directories.documentation,
                module: id,
                config,
            }
            .run()?;
        }

        generate_metadata(state, directories)?;
        Ok(())
    }

    fn run(self) -> Result<(), String> {
        let mut doc = Object::new();
        let name = self.module.name(self.db());
        let file = self.module.file(self.db()).to_string_lossy().into_owned();
        let docs = self.module.documentation(self.db()).to_string();

        doc.add("name", Json::String(name.to_string()));
        doc.add("file", Json::String(file));
        doc.add("documentation", Json::String(docs));
        doc.add("constants", self.constants());
        doc.add("methods", self.module_methods());
        doc.add("types", self.types());
        doc.add("traits", self.traits());

        let path =
            self.directory.join(format!("{}.json", name.normalized_name()));
        let json = Json::Object(doc).to_string();

        write(&path, json)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))
    }

    fn constants(&self) -> Json {
        let mut vals = Vec::new();

        for &id in self.module.constants(self.db()) {
            let public = id.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let name = id.name(self.db()).clone();
            let docs = id.documentation(self.db()).clone();
            let mut obj = Object::new();

            // Constants such as arrays are exposed as references, but we want
            // the type they're defined as, so we force the type to be owned.
            let typ = id.value_type(self.db()).as_owned(self.db());
            let type_name = format!(
                "let{} {}: {}",
                if public { " pub" } else { "" },
                name,
                format_type(self.db(), typ)
            );

            obj.add("name", Json::String(name));
            obj.add("location", location_to_json(id.location(self.db())));
            obj.add("public", Json::Bool(public));
            obj.add("type", Json::String(type_name));
            obj.add("documentation", Json::String(docs));
            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn module_methods(&self) -> Json {
        let mut methods: Vec<MethodId> =
            self.module.extern_methods(self.db()).values().cloned().collect();

        methods.append(&mut self.module.methods(self.db()));
        self.methods(methods)
    }

    fn types(&self) -> Json {
        let mut vals = Vec::new();

        for id in self.module.types(self.db()) {
            let kind = id.kind(self.db());

            if kind.is_closure() || kind.is_module() {
                continue;
            }

            let public = id.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let name = id.name(self.db()).clone();
            let docs = id.documentation(self.db()).clone();
            let is_copy = id.is_copy_type(self.db());
            let is_inline = id.is_inline_type(self.db());
            let mut obj = Object::new();
            let typ = format!(
                "type{}{} {}",
                if public { " pub" } else { "" },
                match kind {
                    TypeKind::Enum if is_copy => " copy enum",
                    TypeKind::Enum if is_inline => " inline enum",
                    TypeKind::Enum => " enum",
                    TypeKind::Async => " async",
                    TypeKind::Extern => " extern",
                    _ if id.is_builtin() => " builtin",
                    _ if is_copy => " copy",
                    _ if is_inline => " inline",
                    _ => "",
                },
                format_type(self.db(), id)
            );

            obj.add("name", Json::String(name));
            obj.add("kind", Json::Int(type_kind(kind, is_copy)));
            obj.add("location", location_to_json(id.location(self.db())));
            obj.add("public", Json::Bool(public));
            obj.add("type", Json::String(typ));
            obj.add("documentation", Json::String(docs));
            obj.add("constructors", self.constructors(id));
            obj.add("fields", self.fields(id));
            obj.add(
                "static_methods",
                self.methods(id.static_methods(self.db())),
            );
            obj.add(
                "instance_methods",
                self.methods(id.instance_methods(self.db())),
            );
            obj.add("implemented_traits", self.implemented_traits(id));

            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn implementations(&self, trait_id: TraitId) -> Json {
        let mut vals = Vec::new();

        for cid in trait_id.implemented_by(self.db()) {
            let imp = cid.trait_implementation(self.db(), trait_id).unwrap();
            let public = cid.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let mut obj = Object::new();
            let tname = cid.name(self.db()).clone();
            let module = cid.module(self.db()).name(self.db()).to_string();
            let mut typ = format!(
                "impl {} for {}",
                format_type(self.db(), imp.instance),
                tname,
            );

            if !imp.bounds.is_empty() {
                typ.push_str(&format_bounds(self.db(), &imp.bounds));
            }

            obj.add("module", Json::String(module));
            obj.add("name", Json::String(tname));
            obj.add("type", Json::String(typ));
            obj.add("public", Json::Bool(public));
            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn implemented_traits(&self, id: TypeId) -> Json {
        let mut vals = Vec::new();

        for imp in id.implemented_traits(self.db()) {
            let trait_id = imp.instance.instance_of();
            let public = trait_id.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let mut obj = Object::new();
            let name = trait_id.name(self.db()).clone();
            let module = trait_id.module(self.db()).name(self.db()).to_string();
            let mut typ = format!(
                "impl {} for {}",
                format_type(self.db(), imp.instance),
                id.name(self.db()),
            );

            if !imp.bounds.is_empty() {
                typ.push_str(&format_bounds(self.db(), &imp.bounds));
            }

            obj.add("module", Json::String(module));
            obj.add("name", Json::String(name));
            obj.add("type", Json::String(typ));
            obj.add("public", Json::Bool(public));
            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn traits(&self) -> Json {
        let mut vals = Vec::new();

        for id in self.module.traits(self.db()) {
            let public = id.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let name = id.name(self.db()).clone();
            let docs = id.documentation(self.db()).clone();
            let mut obj = Object::new();

            obj.add("name", Json::String(name));
            obj.add("location", location_to_json(id.location(self.db())));
            obj.add("public", Json::Bool(public));
            obj.add("type", Json::String(format_type(self.db(), id)));
            obj.add("documentation", Json::String(docs));
            obj.add(
                "required_methods",
                self.methods(id.required_methods(self.db())),
            );
            obj.add(
                "default_methods",
                self.methods(id.default_methods(self.db())),
            );
            obj.add("implementations", self.implementations(id));

            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn methods(&self, methods: Vec<MethodId>) -> Json {
        let mut vals = Vec::new();

        for id in methods {
            let public = id.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let name = id.name(self.db()).clone();
            let kind = id.kind(self.db());

            if id.is_generated(self.db()) || kind.is_constructor() {
                // Generated methods are never included.
                continue;
            }

            let docs = id.documentation(self.db()).clone();
            let file = id.source_file(self.db()).to_string_lossy().into_owned();
            let mut obj = Object::new();
            let typ = format_method(self.db(), id);

            obj.add("name", Json::String(name));
            obj.add("file", Json::String(file));
            obj.add("location", location_to_json(id.location(self.db())));
            obj.add("public", Json::Bool(public));
            obj.add("type", Json::String(typ));
            obj.add("documentation", Json::String(docs));
            vals.push(Json::Object(obj));
        }

        Json::Array(vals)
    }

    fn constructors(&self, id: TypeId) -> Json {
        let mut cons = Vec::new();

        for con in id.constructors(self.db()) {
            let mut obj = Object::new();
            let name = con.name(self.db()).clone();
            let args: Vec<String> = con
                .arguments(self.db())
                .iter()
                .map(|&t| format_type(self.db(), t))
                .collect();

            let typ = format!("{}({})", name, args.join(", "));
            let docs = con.documentation(self.db()).clone();
            let loc = location_to_json(con.location(self.db()));

            obj.add("name", Json::String(name));
            obj.add("location", loc);
            obj.add("type", Json::String(typ));
            obj.add("documentation", Json::String(docs));
            cons.push(Json::Object(obj));
        }

        Json::Array(cons)
    }

    fn fields(&self, id: TypeId) -> Json {
        let mut fields = Vec::new();

        for field in id.fields(self.db()) {
            let public = field.is_public(self.db());

            if self.should_skip(public) {
                continue;
            }

            let mut obj = Object::new();
            let name = field.name(self.db()).clone();
            let mutable = field.is_mutable(self.db());
            let docs = field.documentation(self.db()).clone();
            let loc = location_to_json(field.location(self.db()));
            let typ = format!(
                "let{} @{}: {}",
                if public { " pub" } else { "" },
                name,
                format_type(self.db(), field.value_type(self.db()))
            );

            obj.add("name", Json::String(name));
            obj.add("location", loc);
            obj.add("public", Json::Bool(public));
            obj.add("mutable", Json::Bool(mutable));
            obj.add("type", Json::String(typ));
            obj.add("documentation", Json::String(docs));
            fields.push(Json::Object(obj));
        }

        Json::Array(fields)
    }

    fn db(&self) -> &Database {
        &self.state.db
    }

    fn should_skip(&self, public: bool) -> bool {
        !public && !self.config.private
    }
}

fn generate_metadata(
    state: &State,
    directories: &BuildDirectories,
) -> Result<(), String> {
    let project =
        state.config.source.parent().unwrap_or_else(|| Path::new("."));
    let readme = project.join("README.md");

    // The file name starts with a $ to ensure any documented module names don't
    // conflict with the metadata file, as Inko module names can't include a $.
    let output = directories.documentation.join("$meta.json");
    let mut meta = Object::new();
    let readme_data = if readme.is_file() {
        read_to_string(&readme).map_err(|e| {
            format!("failed to read the README at {}: {}", readme.display(), e)
        })?
    } else {
        String::new()
    };

    meta.add("readme", Json::String(readme_data));

    write(&output, Json::Object(meta).to_string())
        .map_err(|e| format!("failed to write {}: {}", output.display(), e))
}

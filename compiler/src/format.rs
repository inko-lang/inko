use crate::compiler::all_source_modules;
use crate::config::Config;
use crate::diagnostics::{DiagnosticId, Diagnostics};
use crate::hir::Operator;
use ast::nodes::{
    self, ClassExpression, Expression, ImplementationExpression, Node as _,
    Requirement, TopLevelExpression, TraitExpression,
};
use ast::parser::Parser;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::{read, write};
use std::io::{stdin, stdout, Error as IoError, Read as _, Write as _};
use std::path::PathBuf;
use unicode_segmentation::UnicodeSegmentation as _;

/// The characters to use for indentation.
const INDENT: char = ' ';

/// The maximum number of characters (not bytes) per line.
///
/// We use 80 here because:
///
/// 1. It's the most commonly used limit
/// 2. It scales up and down well enough (e.g. you can still fit two vertical
///    splits/windows on a 15" screen). This is important because we don't know
///    anything about the screen size used to view the code
/// 3. At least for prose it's generally considered the ideal line limit
const LIMIT: usize = 80;

#[derive(Clone, Debug)]
enum Node {
    /// A node for which to (recursively) disable wrapping.
    Unwrapped(Box<Node>),

    /// A sequence of nodes to render.
    ///
    /// Unlike Group and Fill, this node itself has no special meaning other
    /// than to serve as a container for multiple nodes.
    Nodes(Vec<Node>),

    /// A group of nodes we try to fit on a single line.
    ///
    /// The arguments are:
    ///
    /// 1. The ID of the group
    /// 2. The list of nodes
    ///
    /// Group should be used instead of Nodes whenever the formatting of the
    /// child nodes may differ based on whether the group as a whole fits on a
    /// single line.
    Group(usize, Vec<Node>),

    /// A group of nodes similar to Group, but when wrapping we try to fit as
    /// many values on a single line as possible.
    Fill(Vec<Node>),

    /// A chunk of ASCII text to display, such as "class" or "async".
    Text(String),

    /// A chunk of text (potentially including Unicode symbols) to display, such
    /// as strings or comments.
    ///
    /// The arguments are:
    ///
    /// 1. The text that may include Unicode characters
    /// 2. The number of grapheme clusters in the string
    Unicode(String, usize),

    /// A node to include if the code is to be wrapped across lines.
    ///
    /// The arguments are:
    ///
    /// 1. The group ID to check
    /// 2. The node to evaluate if wrapping is needed
    /// 3. The node to evaluate if no wrapping is needed, if any
    IfWrap(usize, Box<Node>, Box<Node>),

    /// A node to forcefully wrap if another group is also wrapped.
    ///
    /// For this node to work, it must be processed _before_ the group that
    /// should be wrapped, and _after_ the group to check for if wrapping is
    /// necessary.
    ///
    /// The arguments are:
    ///
    /// 1. The group ID to check
    /// 2. The node to render and optionally wrap
    WrapIf(usize, Box<Node>),

    /// A node that turns into a newline when wrapping is needed, otherwise it
    /// turns into a space.
    SpaceOrLine,

    /// A node that turns into a line when wrapping is needed, and is ignored if
    /// no wrapping is needed.
    Line,

    /// A newline to always insert.
    HardLine,

    /// An empty line to insert without indentation applied to it.
    EmptyLine,

    /// Forces wrapping of the surrounding group.
    WrapParent,

    /// Indent the given nodes (recursively), but only if wrapping is necessary.
    Indent(Vec<Node>),

    /// A node of which the width should be reported as zero.
    ZeroWidth(Box<Node>),

    /// Indent the given nodes recursively, but only starting the next line.
    IndentNext(Vec<Node>),

    /// A node that represents a method call chain.
    ///
    /// The arguments are as follows:
    ///
    /// 1. The group ID
    /// 2. The "head" of the call chain. This is the initial receiver and any
    ///    intermediate calls (e.g. `foo.bar` in `foo.bar.baz(...)`).
    /// 3. The "middle" part of the call chain. This is the last method called,
    ///    minus its arguments (e.g. `baz` in `foo.bar.baz(...)`)
    /// 4. The "tail", which includes the arguments of the last call in the
    ///    chain
    Call(usize, Box<Node>, Box<Node>, Option<Box<Node>>),
}

impl Node {
    fn text(value: &str) -> Node {
        Node::Text(value.to_string())
    }

    fn unicode(text: String) -> Node {
        let width = text.graphemes(true).count();

        Node::Unicode(text, width)
    }

    fn width(&self, wrapped: &HashSet<usize>, force: bool) -> usize {
        // We use a recursive implementation so we can special-case the width
        // calculation more easily for certain nodes (e.g. Call nodes). Unless
        // somebody writes really bizarre code, this shouldn't blow up the call
        // stack.
        match self {
            Node::Nodes(n) => n.iter().map(|n| n.width(wrapped, force)).sum(),
            Node::Group(_, n) => {
                n.iter().map(|n| n.width(wrapped, force)).sum()
            }
            Node::Fill(n) => n.iter().map(|n| n.width(wrapped, force)).sum(),
            Node::Unwrapped(n) => n.width(wrapped, force),
            Node::Indent(n) => n.iter().map(|n| n.width(wrapped, force)).sum(),
            Node::IndentNext(n) => {
                n.iter().map(|n| n.width(wrapped, force)).sum()
            }
            Node::Text(v) | Node::Unicode(v, _) if v.contains('\n') => LIMIT,
            Node::Text(v) => v.len(),
            Node::Unicode(_, w) => *w,
            Node::SpaceOrLine => 1,
            Node::IfWrap(id, n, _) if wrapped.contains(id) => {
                n.width(wrapped, force)
            }
            Node::IfWrap(_, _, n) => n.width(wrapped, force),
            Node::WrapIf(_, n) => n.width(wrapped, force),
            Node::HardLine | Node::EmptyLine | Node::WrapParent => LIMIT,
            Node::Call(_, head, mid, tail) => {
                head.width(wrapped, true)
                    + mid.width(wrapped, true)
                    + tail.as_ref().map_or(0, |n| n.width(wrapped, true))
            }
            Node::ZeroWidth(n) if force => n.width(wrapped, force),
            _ => 0,
        }
    }
}

enum Output {
    Stdout,
    File,
}

struct InputIterator {
    input: Input,
    index: usize,
}

impl Iterator for InputIterator {
    type Item = Result<(PathBuf, Vec<u8>, Output), (PathBuf, IoError)>;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.input {
            Input::Stdin if self.index == 0 => {
                self.index += 1;

                let mut data = Vec::new();
                let path = PathBuf::from("STDIN");

                match stdin().read_to_end(&mut data) {
                    Ok(_) => Some(Ok((path, data, Output::Stdout))),
                    Err(e) => Some(Err((path, e))),
                }
            }
            Input::Files(paths) if self.index < paths.len() => {
                let path = paths[self.index].clone();

                self.index += 1;

                match read(&path) {
                    Ok(data) => Some(Ok((path, data, Output::File))),
                    Err(e) => Some(Err((path, e))),
                }
            }
            _ => None,
        }
    }
}

pub enum Input {
    /// Format data passed using STDIN, and write the results to STDOUT.
    Stdin,

    /// Format the given files and update them in place.
    Files(Vec<PathBuf>),
}

impl Input {
    pub fn project(config: &Config) -> Result<Input, String> {
        let files = all_source_modules(config, true)?
            .into_iter()
            .map(|(_, p)| p)
            .collect();

        Ok(Input::Files(files))
    }

    fn into_iter(self) -> InputIterator {
        InputIterator { input: self, index: 0 }
    }
}

pub enum Error {
    Internal(String),
    Diagnostics,
}

/// A type for formatting Inko source files.
pub struct Formatter {
    config: Config,
    diagnostics: Diagnostics,
}

impl Formatter {
    pub fn new(config: Config) -> Formatter {
        Formatter { config, diagnostics: Diagnostics::new() }
    }

    pub fn check(&mut self, input: Input) -> Result<Vec<PathBuf>, Error> {
        let mut incorrect = Vec::new();

        for result in input.into_iter() {
            let (path, data, _) = result.map_err(|(path, e)| {
                Error::Internal(format!(
                    "failed to read {}: {}",
                    path.display(),
                    e
                ))
            })?;

            let orig = String::from_utf8_lossy(&data).into_owned();
            let mut parser = Parser::with_comments(data, path.clone());
            let ast = match parser.parse() {
                Ok(v) => v,
                Err(e) => {
                    self.diagnostics.error(
                        DiagnosticId::InvalidSyntax,
                        e.message,
                        path.clone(),
                        e.location,
                    );

                    continue;
                }
            };

            if Document::new().format(ast) != orig {
                incorrect.push(path);
            }
        }

        if self.diagnostics.has_errors() {
            Err(Error::Diagnostics)
        } else {
            Ok(incorrect)
        }
    }

    pub fn format(&mut self, input: Input) -> Result<(), Error> {
        for result in input.into_iter() {
            let (path, data, out) = result.map_err(|(path, e)| {
                Error::Internal(format!(
                    "failed to read {}: {}",
                    path.display(),
                    e
                ))
            })?;

            let mut parser = Parser::with_comments(data, path.clone());
            let ast = match parser.parse() {
                Ok(v) => v,
                Err(e) => {
                    self.diagnostics.error(
                        DiagnosticId::InvalidSyntax,
                        e.message,
                        path.clone(),
                        e.location,
                    );

                    continue;
                }
            };

            let res = Document::new().format(ast);

            match out {
                Output::Stdout => {
                    stdout().lock().write_all(res.as_bytes()).map_err(|e| {
                        Error::Internal(format!(
                            "failed to write to STDOUT: {}",
                            e
                        ))
                    })
                }
                Output::File => write(&path, res.as_bytes()).map_err(|e| {
                    Error::Internal(format!(
                        "failed to update {}: {}",
                        path.display(),
                        e
                    ))
                }),
            }?;
        }

        if self.diagnostics.has_errors() {
            Err(Error::Diagnostics)
        } else {
            Ok(())
        }
    }

    pub fn print_diagnostics(&self) {
        self.config.presenter.present(&self.diagnostics);
    }
}

struct Import<'a> {
    path: String,
    symbols: Option<Vec<(&'a String, Option<&'a String>)>>,
    tags: Option<Vec<&'a String>>,
}

struct List {
    group_id: usize,
    capacity: usize,
    size: usize,
    nodes: Vec<Node>,
}

impl List {
    fn new(group_id: usize, capacity: usize) -> List {
        List { group_id, capacity, size: 0, nodes: Vec::new() }
    }

    fn add(&mut self, node: Node) {
        self.nodes.push(node);
    }

    fn push(&mut self, id: usize, node: Node, comma: bool, separator: Node) {
        self.size += 1;

        // If the group as a whole (including the separator) doesn't fit on a
        // single line, we need to force wrapping of the value, even if the
        // value itself fits on the line.
        let mut group = vec![Node::WrapIf(id, Box::new(node))];
        let remaining = self.size < self.capacity;

        // The separator needs to be grouped together with the expression such
        // that the width calculation takes the separator into account.
        if comma && remaining {
            group.push(Node::text(","));
        } else if comma && self.size == self.capacity {
            group.push(Node::IfWrap(
                self.group_id,
                Box::new(Node::text(",")),
                Box::new(Node::text("")),
            ));
        }

        self.add(Node::Group(id, group));

        if remaining {
            self.add(separator);
        }
    }

    fn into_nodes(self) -> Vec<Node> {
        self.nodes
    }
}

struct Document {
    gen: Generator,
    group_id: usize,
}

impl Document {
    fn new() -> Document {
        Document { gen: Generator::new(), group_id: 0 }
    }

    fn format(mut self, ast: nodes::Module) -> String {
        self.top_level(&ast);
        self.gen.buf
    }

    fn top_level(&mut self, ast: &nodes::Module) {
        let mut iter = ast.expressions.iter().peekable();

        while let Some(node) = iter.next() {
            match node {
                TopLevelExpression::Import(n) => {
                    let mut imports = vec![&**n];

                    // This groups consecutive imports together so we can sort
                    // them.
                    while let Some(TopLevelExpression::Import(n)) = iter.peek()
                    {
                        imports.push(n);
                        iter.next();
                    }

                    self.imports(imports);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::ExternImport(n) => {
                    let mut imports = vec![&**n];

                    while let Some(TopLevelExpression::ExternImport(n)) =
                        iter.peek()
                    {
                        imports.push(n);
                        iter.next();
                    }

                    self.extern_imports(imports);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::Comment(c) => {
                    self.top_level_comment(c);

                    // If a comment is followed by an empty line we retain the
                    // empty line. This allows one to create a module comment
                    // followed by e.g. a type, without that comment being
                    // turned into a comment for the _type_ instead of the
                    // module.
                    if let Some(node) = iter.peek() {
                        if node.location().lines.start()
                            - c.location.lines.end()
                            > 1
                        {
                            self.gen.new_line();
                        }
                    }
                }
                TopLevelExpression::DefineConstant(n) => {
                    self.define_constant(n);

                    match iter.peek() {
                        Some(TopLevelExpression::Comment(c))
                            if c.location.is_trailing(&n.location) =>
                        {
                            self.gen.single_space();
                        }
                        Some(TopLevelExpression::DefineConstant(_)) | None => {
                            self.gen.new_line();
                        }
                        Some(_) => {
                            self.gen.new_line();
                            self.gen.new_line();
                        }
                    }
                }
                TopLevelExpression::DefineMethod(n) => {
                    let node = self.define_method(n);

                    self.gen.generate(node);
                    self.gen.new_line();

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::DefineClass(n) => {
                    self.define_class(n);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::DefineTrait(n) => {
                    self.define_trait(n);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::ReopenClass(n) => {
                    self.reopen_class(n);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
                TopLevelExpression::ImplementTrait(n) => {
                    self.implement_trait(n);

                    if iter.peek().is_some() {
                        self.gen.new_line();
                    }
                }
            }
        }
    }

    fn imports(&mut self, nodes: Vec<&nodes::Import>) {
        let mut imports = Vec::new();

        for node in nodes {
            let path = node
                .path
                .steps
                .iter()
                .map(|i| i.name.clone())
                .collect::<Vec<_>>()
                .join(".");

            let symbols = node.symbols.as_ref().map(|v| {
                v.values
                    .iter()
                    .map(|s| (&s.name, s.alias.as_ref().map(|v| &v.name)))
                    .collect::<Vec<_>>()
            });

            let tags = node
                .tags
                .as_ref()
                .map(|v| v.values.iter().map(|t| &t.name).collect::<Vec<_>>());

            imports.push(Import { path, symbols, tags });
        }

        // Imports are sorted alphabetically based on their paths.
        imports.sort_by(|a, b| a.path.cmp(&b.path));

        for import in &mut imports {
            if let Some(syms) = &mut import.symbols {
                syms.sort_by(|(a, _), (b, _)| {
                    // "self" should always be the first symbol.
                    if *a == "self" {
                        Ordering::Less
                    } else if *b == "self" {
                        Ordering::Greater
                    } else {
                        a.cmp(b)
                    }
                });
            }
        }

        let max = imports.len() - 1;

        for (idx, import) in imports.into_iter().enumerate() {
            let mut nodes =
                vec![Node::text("import "), Node::Text(import.path)];
            let syms_id = self.new_group_id();

            // Symbols are formatted in one of three ways:
            //
            // 1. They're placed on the same line as the `import`, if this fits
            // 2. They're placed on a separate line, if they all fit on the same
            //    line
            // 3. Each symbol is placed on its own line
            if let Some(pairs) = import.symbols {
                let vals = self.list(&pairs, syms_id, |_, (name, alias)| {
                    let val = if let Some(alias) = alias {
                        vec![
                            Node::text(name),
                            Node::text(" as "),
                            Node::text(alias),
                        ]
                    } else {
                        vec![Node::text(name)]
                    };

                    Node::Nodes(val)
                });

                let group = vec![
                    Node::text(" ("),
                    Node::Line,
                    Node::Indent(vec![Node::Fill(vals)]),
                    Node::Line,
                    Node::text(")"),
                ];

                nodes.push(Node::Group(syms_id, group));
            }

            if let Some(tag_names) = import.tags {
                let mut group = vec![Node::SpaceOrLine, Node::text("if")];
                let mut tags = vec![Node::SpaceOrLine];

                for (idx, tag) in tag_names.into_iter().enumerate() {
                    let mut pair = Vec::new();

                    if idx > 0 {
                        tags.push(Node::SpaceOrLine);
                        pair.push(Node::text("and "));
                    }

                    pair.push(Node::text(tag));

                    if idx > 0 {
                        tags.push(Node::Indent(pair))
                    } else {
                        tags.push(Node::Nodes(pair))
                    }
                }

                group.push(self.group(vec![Node::Indent(tags)]));

                // If the `import` is followed by another import and the tags
                // don't fit on a single line, we insert an empty line in
                // between such that it's more clear the `if` applies to the
                // _current_ import and not the next one.
                if idx < max {
                    group.push(Node::Line);
                }

                // If the list of symbols doesn't fit on a single line, we also
                // wrap the tags, otherwise the list can be difficult to read.
                nodes.push(Node::WrapIf(syms_id, Box::new(self.group(group))));
            }

            self.gen.generate(Node::Nodes(nodes));
            self.gen.new_line();
        }
    }

    fn extern_imports(&mut self, mut nodes: Vec<&nodes::ExternImport>) {
        nodes.sort_by(|a, b| a.path.path.cmp(&b.path.path));

        for import in nodes {
            let path = format!("\"{}\"", import.path.path);
            let node = Node::Nodes(vec![
                Node::text("import extern "),
                Node::unicode(path),
            ]);

            self.gen.generate(node);
            self.gen.new_line();
        }
    }

    fn top_level_comment(&mut self, node: &nodes::Comment) {
        let group = self.comment(node);

        self.gen.generate(group);
        self.gen.new_line();
    }

    fn comment(&mut self, node: &nodes::Comment) -> Node {
        let mut nodes = vec![Node::text("#")];

        if !node.value.is_empty() {
            nodes.push(Node::text(" "));
            nodes.push(Node::unicode(node.value.clone()));
        }

        nodes.push(Node::WrapParent);
        Node::Nodes(nodes)
    }

    fn define_constant(&mut self, node: &nodes::DefineConstant) {
        let kw = if node.public { "let pub " } else { "let " };
        let val = self.expression(&node.value);
        let nodes = Node::Nodes(vec![
            Node::Text(kw.to_string()),
            Node::Text(node.name.name.clone()),
            Node::text(" = "),
            val,
        ]);

        self.gen.generate(nodes);
    }

    fn define_class(&mut self, node: &nodes::DefineClass) {
        let header_id = self.new_group_id();
        let mut header = vec![Node::text("class ")];

        if node.public {
            header.push(Node::text("pub "));
        }

        match node.kind {
            nodes::ClassKind::Async => header.push(Node::text("async ")),
            nodes::ClassKind::Builtin => header.push(Node::text("builtin ")),
            nodes::ClassKind::Enum => header.push(Node::text("enum ")),
            nodes::ClassKind::Extern => header.push(Node::text("extern ")),
            nodes::ClassKind::Regular => {}
        }

        header.push(Node::text(&node.name.name));

        if let Some(nodes) =
            node.type_parameters.as_ref().filter(|v| !v.values.is_empty())
        {
            header.push(self.type_parameters(&nodes.values));
        }

        header.push(Node::text(" {"));

        let mut iter = node.body.values.iter().peekable();
        let mut exprs = Vec::new();

        while let Some(expr) = iter.next() {
            let trailing = match iter.peek() {
                Some(ClassExpression::Comment(next))
                    if next.location.is_trailing(expr.location()) =>
                {
                    iter.next();
                    Some(self.comment(next))
                }
                _ => None,
            };

            let next = iter.peek();
            let (node, tight) = match expr {
                ClassExpression::DefineMethod(n) => {
                    (self.define_method(n), false)
                }
                ClassExpression::DefineField(n) => (
                    self.define_field(n),
                    matches!(next, Some(ClassExpression::DefineField(_))),
                ),
                ClassExpression::DefineVariant(n) => (
                    self.define_variant(n),
                    matches!(next, Some(ClassExpression::DefineVariant(_))),
                ),
                ClassExpression::Comment(n) => (self.comment(n), true),
            };

            exprs.push(node);

            if let Some(node) = trailing {
                exprs.push(Node::text(" "));
                exprs.push(node);
            }

            if iter.peek().is_some() {
                exprs.push(if tight {
                    Node::HardLine
                } else {
                    Node::EmptyLine
                });
            }
        }

        let body = if exprs.is_empty() {
            vec![Node::Line, Node::text("}")]
        } else {
            vec![
                Node::HardLine,
                Node::Indent(exprs),
                Node::HardLine,
                Node::text("}"),
            ]
        };

        let class = vec![
            Node::Group(header_id, header),
            Node::WrapIf(header_id, Box::new(self.group(body))),
        ];

        self.gen.generate(Node::Nodes(class));
        self.gen.new_line();
    }

    fn define_trait(&mut self, node: &nodes::DefineTrait) {
        let header_id = self.new_group_id();
        let mut header = vec![Node::text("trait ")];

        if node.public {
            header.push(Node::text("pub "));
        }

        header.push(Node::text(&node.name.name));

        if let Some(nodes) =
            node.type_parameters.as_ref().filter(|v| !v.values.is_empty())
        {
            header.push(self.type_parameters(&nodes.values));
        }

        if let Some(nodes) = &node.requirements {
            let mut reqs = Vec::new();

            for (idx, node) in nodes.values.iter().enumerate() {
                let mut pair = Vec::new();

                if idx > 0 {
                    reqs.push(Node::SpaceOrLine);
                    pair.push(Node::text("+ "));
                }

                pair.push(self.type_name(node, None));

                if idx > 0 {
                    reqs.push(Node::Indent(pair));
                } else {
                    reqs.push(Node::Nodes(pair));
                }
            }

            header.push(Node::text(": "));
            header.push(Node::Nodes(reqs));
        }

        header.push(Node::SpaceOrLine);
        header.push(Node::text("{"));

        let mut iter = node.body.values.iter().peekable();
        let mut exprs = Vec::new();

        while let Some(expr) = iter.next() {
            let trailing = match iter.peek() {
                Some(TraitExpression::Comment(next))
                    if next.location.is_trailing(expr.location()) =>
                {
                    iter.next();
                    Some(self.comment(next))
                }
                _ => None,
            };

            let (node, tight) = match expr {
                TraitExpression::DefineMethod(n) => {
                    (self.define_method(n), false)
                }
                TraitExpression::Comment(n) => (self.comment(n), true),
            };

            exprs.push(node);

            if let Some(node) = trailing {
                exprs.push(Node::text(" "));
                exprs.push(node);
            }

            if iter.peek().is_some() {
                exprs.push(if tight {
                    Node::HardLine
                } else {
                    Node::EmptyLine
                });
            }
        }

        let body = if exprs.is_empty() {
            vec![Node::Line, Node::text("}")]
        } else {
            vec![
                Node::HardLine,
                Node::Indent(exprs),
                Node::HardLine,
                Node::text("}"),
            ]
        };

        let group = vec![
            Node::Group(header_id, header),
            Node::WrapIf(header_id, Box::new(self.group(body))),
        ];

        self.gen.generate(Node::Nodes(group));
        self.gen.new_line();
    }

    fn reopen_class(&mut self, node: &nodes::ReopenClass) {
        let header_id = self.new_group_id();
        let mut header =
            vec![Node::text("impl "), Node::text(&node.class_name.name)];

        if let Some(node) = &node.bounds {
            header.push(self.type_bounds(node));
        }

        header.push(Node::SpaceOrLine);
        header.push(Node::text("{"));

        let body = self.implementation(&node.body);
        let group = vec![
            Node::Group(header_id, header),
            Node::WrapIf(header_id, Box::new(self.group(body))),
        ];

        self.gen.generate(Node::Nodes(group));
        self.gen.new_line();
    }

    fn implement_trait(&mut self, node: &nodes::ImplementTrait) {
        let header_id = self.new_group_id();
        let start = vec![
            Node::text("impl "),
            self.type_name(&node.trait_name, None),
            Node::SpaceOrLine,
            Node::text("for "),
            Node::text(&node.class_name.name),
        ];
        let mut header = vec![self.group(start)];

        if let Some(node) = &node.bounds {
            header.push(self.type_bounds(node));
        }

        header.push(Node::SpaceOrLine);
        header.push(Node::text("{"));

        let body = self.implementation(&node.body);
        let group = vec![
            Node::Group(header_id, header),
            Node::WrapIf(header_id, Box::new(self.group(body))),
        ];

        self.gen.generate(Node::Nodes(group));
        self.gen.new_line();
    }

    fn type_bounds(&mut self, node: &nodes::TypeBounds) -> Node {
        let gid = self.new_group_id();
        let vals = self.list(&node.values, gid, |this, node| {
            let pair = vec![
                Node::text(&node.name.name),
                Node::text(": "),
                this.type_parameter_requirements(&node.requirements),
            ];

            Node::Nodes(pair)
        });
        let group = vec![
            Node::SpaceOrLine,
            Node::text("if"),
            Node::SpaceOrLine,
            Node::Indent(vals),
        ];

        Node::Group(gid, group)
    }

    fn implementation(
        &mut self,
        nodes: &nodes::ImplementationExpressions,
    ) -> Vec<Node> {
        let mut iter = nodes.values.iter().peekable();
        let mut exprs = Vec::new();

        while let Some(expr) = iter.next() {
            let trailing = match iter.peek() {
                Some(ImplementationExpression::Comment(next))
                    if next.location.is_trailing(expr.location()) =>
                {
                    iter.next();
                    Some(self.comment(next))
                }
                _ => None,
            };

            let (node, tight) = match expr {
                ImplementationExpression::DefineMethod(n) => {
                    (self.define_method(n), false)
                }
                ImplementationExpression::Comment(n) => (self.comment(n), true),
            };

            exprs.push(node);

            if let Some(node) = trailing {
                exprs.push(Node::text(" "));
                exprs.push(node);
            }

            if iter.peek().is_some() {
                exprs.push(if tight {
                    Node::HardLine
                } else {
                    Node::EmptyLine
                });
            }
        }

        if exprs.is_empty() {
            vec![Node::Line, Node::text("}")]
        } else {
            vec![
                Node::HardLine,
                Node::Indent(exprs),
                Node::HardLine,
                Node::text("}"),
            ]
        }
    }

    fn define_field(&mut self, node: &nodes::DefineField) -> Node {
        let mut group = vec![Node::text("let ")];

        if node.public {
            group.push(Node::text("pub "));
        }

        group.push(Node::text(&format!("@{}", node.name.name)));
        group.push(Node::text(": "));
        group.push(self.type_reference(&node.value_type));
        Node::Nodes(group)
    }

    fn define_variant(&mut self, node: &nodes::DefineVariant) -> Node {
        let mut group = vec![Node::text("case "), Node::text(&node.name.name)];

        if let Some(nodes) =
            node.members.as_ref().filter(|v| !v.values.is_empty())
        {
            let gid = self.new_group_id();
            let vals =
                self.list(&nodes.values, gid, |s, n| s.type_reference(n));
            let args = self.argument_list(vals);

            group.push(Node::Group(gid, args));
        }

        Node::Nodes(group)
    }

    fn define_method(&mut self, node: &nodes::DefineMethod) -> Node {
        let header_id = self.new_group_id();
        let kind = match node.kind {
            nodes::MethodKind::Instance => " ",
            nodes::MethodKind::Static => " static ",
            nodes::MethodKind::Async => " async ",
            nodes::MethodKind::Moving => " move ",
            nodes::MethodKind::Mutable => " mut ",
            nodes::MethodKind::AsyncMutable => " async mut ",
            nodes::MethodKind::Extern => " extern ",
        };
        let kw = if node.public { "fn pub" } else { "fn" };
        let mut header =
            vec![Node::text(kw), Node::text(kind), Node::text(&node.name.name)];

        if let Some(nodes) =
            node.type_parameters.as_ref().filter(|v| !v.values.is_empty())
        {
            header.push(self.type_parameters(&nodes.values));
        }

        if let Some(nodes) =
            node.arguments.as_ref().filter(|v| !v.values.is_empty())
        {
            let mut args = nodes
                .values
                .iter()
                .map(|n| (n.name.name.as_str(), Some(&n.value_type)))
                .collect::<Vec<_>>();

            if nodes.variadic {
                args.push(("...", None));
            }

            let vals = self.list(&args, header_id, |this, (name, typ)| {
                let mut pair = vec![Node::text(name)];

                if let Some(typ) = typ {
                    pair.push(Node::text(": "));
                    pair.push(this.type_reference(typ));
                }

                Node::Nodes(pair)
            });

            header.push(Node::Nodes(self.argument_list(vals)));

            if let Some(rnode) = &node.return_type {
                header.push(self.return_type(rnode));
            }
        } else if let Some(rnode) = &node.return_type {
            header.push(self.return_type(rnode));
        }

        if node.body.is_some() {
            header.push(Node::text(" {"));
        }

        let mut method = vec![Node::Group(header_id, header)];

        if let Some(node) = &node.body {
            let body = self.method_body(&node.values);

            method.push(Node::WrapIf(header_id, Box::new(self.group(body))));
        }

        Node::Nodes(method)
    }

    fn argument_list(&mut self, nodes: Vec<Node>) -> Vec<Node> {
        vec![
            Node::text("("),
            Node::Line,
            Node::Indent(nodes),
            Node::Line,
            Node::text(")"),
        ]
    }

    fn type_parameters(&mut self, nodes: &[nodes::TypeParameter]) -> Node {
        let gid = self.new_group_id();
        let vals = self.list(nodes, gid, |s, n| s.type_parameter(n));
        let group = vec![
            Node::text("["),
            Node::Line,
            Node::Indent(vals),
            Node::Line,
            Node::text("]"),
        ];

        Node::Group(gid, group)
    }

    fn return_type(&mut self, node: &nodes::Type) -> Node {
        Node::Nodes(vec![Node::text(" -> "), self.type_reference(node)])
    }

    fn method_body(&mut self, nodes: &[Expression]) -> Vec<Node> {
        if nodes.is_empty() {
            vec![Node::Line, Node::text("}")]
        } else {
            vec![
                Node::HardLine,
                Node::Indent(self.expressions(nodes)),
                Node::HardLine,
                Node::text("}"),
            ]
        }
    }

    fn body(&mut self, nodes: &[Expression]) -> Vec<Node> {
        if nodes.is_empty() {
            vec![Node::Line, Node::text("}")]
        } else {
            vec![
                Node::SpaceOrLine,
                Node::Indent(self.expressions(nodes)),
                Node::SpaceOrLine,
                Node::text("}"),
            ]
        }
    }

    fn expressions(&mut self, nodes: &[Expression]) -> Vec<Node> {
        let mut vals = Vec::new();
        let mut iter = nodes.iter().peekable();

        while let Some(expr) = iter.next() {
            let trailing = match iter.peek() {
                Some(Expression::Comment(next))
                    if next.location.is_trailing(expr.location()) =>
                {
                    iter.next();
                    Some(self.comment(next))
                }
                _ => None,
            };

            vals.push(self.expression(expr));

            if let Some(node) = trailing {
                vals.push(Node::text(" "));
                vals.push(node);
            }

            if let Some(next) = iter.peek() {
                let sep = if next.location().lines.start()
                    - expr.location().lines.end()
                    > 1
                {
                    // Multiple empty lines are condensed into a single empty
                    // line. This keeps the code style consistent, while still
                    // giving users some option to group code together in a way
                    // they deem to be better (e.g. by separating method calls
                    // with an empty line).
                    Node::EmptyLine
                } else {
                    match (expr, next) {
                        (Expression::Comment(_), _) => Node::HardLine,
                        // Conditionals are surrounded by an empty line as to
                        // make them stand out more.
                        _ if expr.is_conditional() || next.is_conditional() => {
                            Node::EmptyLine
                        }
                        // `let` and comments are grouped together.
                        (
                            Expression::DefineVariable(_),
                            Expression::DefineVariable(_)
                            | Expression::Comment(_),
                        ) => Node::HardLine,
                        // `let` followed by anything else, or something
                        // followed by a `let` is separated by an empty line.
                        (Expression::DefineVariable(_), _)
                        | (_, Expression::DefineVariable(_)) => Node::EmptyLine,
                        _ => Node::HardLine,
                    }
                };

                vals.push(sep);
            } else if nodes.len() == 1 && expr.is_conditional() {
                // Conditionals inside bodies are a bit difficult to read due to
                // all the curly braces, so for expressions such as
                // `if foo { loop { ... } }` we force wrapping across lines.
                vals.push(Node::WrapParent);
            }
        }

        vals
    }

    fn expression(&mut self, node: &Expression) -> Node {
        match node {
            Expression::Int(n) => Node::text(&n.value),
            Expression::Float(n) => Node::text(&n.value),
            Expression::True(_) => Node::text("true"),
            Expression::False(_) => Node::text("false"),
            Expression::String(n) => self.string_literal(n),
            Expression::Array(n) => self.array(n),
            Expression::Binary(_) => self.binary(node),
            Expression::And(_) => self.and_or(node),
            Expression::Or(_) => self.and_or(node),
            Expression::Constant(n) => self.constant(n),
            Expression::Comment(n) => self.comment(n),
            Expression::DefineVariable(n) => self.define_variable(n),
            Expression::While(n) => self.conditional_loop(n),
            Expression::If(n) => self.if_else(n),
            Expression::Group(n) => self.grouped_expression(n),
            Expression::Field(n) => Node::text(&format!("@{}", n.name)),
            Expression::Identifier(n) => Node::text(&n.name),
            Expression::AssignVariable(n) => self.assign_variable(n),
            Expression::ReplaceVariable(n) => self.replace_variable(n),
            Expression::AssignField(n) => self.assign_field(n),
            Expression::ReplaceField(n) => self.replace_field(n),
            Expression::AssignSetter(n) => self.assign_setter(n),
            Expression::BinaryAssignVariable(n) => {
                self.binary_assign_variable(n)
            }
            Expression::BinaryAssignField(n) => self.binary_assign_field(n),
            Expression::BinaryAssignSetter(n) => self.binary_assign_setter(n),
            Expression::SelfObject(_) => Node::text("self"),
            Expression::Next(_) => Node::text("next"),
            Expression::Break(_) => Node::text("break"),
            Expression::Ref(n) => self.reference("ref", &n.value),
            Expression::Mut(n) => self.reference("mut", &n.value),
            Expression::Recover(n) => self.recover(n),
            Expression::Throw(n) => self.throw_value(n),
            Expression::Return(n) => self.return_value(n),
            Expression::Loop(n) => self.unconditional_loop(n),
            Expression::Nil(_) => Node::text("nil"),
            Expression::Tuple(n) => self.tuple(n),
            Expression::Call(_) => self.call(node),
            Expression::Closure(n) => self.closure(n, false),
            Expression::TypeCast(n) => self.type_cast(n),
            Expression::Try(n) => self.try_value(n),
            Expression::Match(n) => self.match_value(n),
            Expression::ClassLiteral(n) => self.class_literal(n),
            Expression::Scope(n) => self.scope(n),
        }
    }

    fn constant(&mut self, node: &nodes::Constant) -> Node {
        self.group(self.constant_name(node))
    }

    fn constant_name(&self, node: &nodes::Constant) -> Vec<Node> {
        if let Some(src) = &node.source {
            vec![
                Node::text(&src.name),
                Node::Line,
                Node::Indent(vec![Node::text("."), Node::text(&node.name)]),
            ]
        } else {
            vec![Node::text(&node.name)]
        }
    }

    fn binary(&mut self, node: &Expression) -> Node {
        let mut ops = Vec::new();
        let mut first_node = node;

        // This "unrolls" nested binary expressions into a flat list.
        loop {
            match first_node {
                Expression::Binary(n) => {
                    first_node = &n.left;
                    ops.push((n.operator.kind, &n.right));
                }
                node => {
                    first_node = node;
                    break;
                }
            }
        }

        let head = self.expression(first_node);
        let mut tail = Vec::new();

        while let Some((op, expr)) = ops.pop() {
            let sym = Operator::from_ast(op).method_name();

            if !tail.is_empty() {
                tail.push(Node::SpaceOrLine);
            }

            tail.push(Node::text(sym));
            tail.push(Node::text(" "));
            tail.push(self.expression(expr));
        }

        self.group(vec![head, Node::SpaceOrLine, Node::Indent(tail)])
    }

    fn and_or(&mut self, node: &nodes::Expression) -> Node {
        let mut ops = Vec::new();
        let mut first_node = node;

        loop {
            match first_node {
                Expression::And(n) => {
                    first_node = &n.left;
                    ops.push(("and", &n.right));
                }
                Expression::Or(n) => {
                    first_node = &n.left;
                    ops.push(("or", &n.right));
                }
                node => {
                    first_node = node;
                    break;
                }
            }
        }

        let head = self.expression(first_node);
        let mut tail = Vec::new();

        while let Some((op, expr)) = ops.pop() {
            if !tail.is_empty() {
                tail.push(Node::SpaceOrLine);
            }

            tail.push(Node::text(op));
            tail.push(Node::text(" "));
            tail.push(self.expression(expr));
        }

        self.group(vec![head, Node::SpaceOrLine, Node::Indent(tail)])
    }

    fn array(&mut self, node: &nodes::Array) -> Node {
        let mut nodes = vec![Node::text("["), Node::Line];
        let gid = self.new_group_id();

        // For these sort of arrays we fit as many values on a single line as
        // possible, reducing the number of lines necessary.
        //
        // We don't do this for strings as their lengths can vary greatly,
        // resulting in weird wrapping, like so:
        //
        //     [
        //       'short string value',
        //       'really long string value here ......',
        //       'foo', 'bar',
        //     ]
        let fill = node.values.iter().all(|expr| {
            matches!(
                expr,
                Expression::Int(_)
                    | Expression::Float(_)
                    | Expression::True(_)
                    | Expression::False(_)
                    | Expression::Constant(_)
                    | Expression::Comment(_)
            )
        });

        let mut list = List::new(gid, node.values.len());
        let mut iter = node.values.iter().peekable();

        while let Some(n) = iter.next() {
            let id = self.new_group_id();
            let comment = matches!(n, Expression::Comment(_));
            let trailing_comment = iter
                .peek()
                .map_or(false, |v| v.is_trailing_comment(n.location()));

            let sep = if trailing_comment {
                Node::text(" ")
            } else if comment {
                Node::HardLine
            } else if matches!(iter.peek(), Some(Expression::Comment(_))) {
                Node::EmptyLine
            } else {
                Node::SpaceOrLine
            };

            list.push(id, self.expression(n), !comment, sep);
        }

        let result = if fill {
            Node::Indent(vec![Node::Fill(list.into_nodes())])
        } else {
            Node::Indent(list.into_nodes())
        };

        nodes.push(result);
        nodes.push(Node::Line);
        nodes.push(Node::text("]"));
        Node::Group(gid, nodes)
    }

    fn string_literal(&mut self, node: &nodes::StringLiteral) -> Node {
        let mut vals = Vec::new();
        let dquote = node.values.iter().any(|n| match n {
            nodes::StringValue::Text(n) => n.value.contains('\''),
            _ => false,
        });

        for n in &node.values {
            match n {
                nodes::StringValue::Text(n) => {
                    vals.push(Node::unicode(n.value.clone()));
                }
                nodes::StringValue::Escape(n) => {
                    let text = match n.value.chars().next().unwrap() {
                        '\t' => "\\t".to_string(),
                        '\r' => "\\r".to_string(),
                        '\n' => "\\n".to_string(),
                        '\0' => "\\0".to_string(),
                        '\u{1b}' => "\\e".to_string(),
                        '\\' => "\\\\".to_string(),
                        '\'' => if dquote { "'" } else { "\\'" }.to_string(),
                        '"' => if dquote { "\\\"" } else { "\"" }.to_string(),
                        '$' => "\\$".to_string(),
                        char => format!("\\u{{{:X}}}", char as u32),
                    };

                    vals.push(Node::Text(text));
                }
                nodes::StringValue::Expression(n) => {
                    vals.push(Node::text("${"));
                    vals.push(Node::Unwrapped(Box::new(
                        self.expression(&n.value),
                    )));
                    vals.push(Node::text("}"));
                }
            }
        }

        let quote = if dquote { "\"" } else { "'" };

        Node::Nodes(vec![
            Node::text(quote),
            Node::Nodes(vals),
            Node::text(quote),
        ])
    }

    fn define_variable(&mut self, node: &nodes::DefineVariable) -> Node {
        let kw = if node.mutable { "let mut " } else { "let " };
        let mut var = vec![
            Node::Text(kw.to_string()),
            Node::Text(node.name.name.clone()),
        ];

        if let Some(n) = &node.value_type {
            var.push(Node::text(": "));
            var.push(self.type_reference(n));
        }

        var.push(Node::text(" = "));
        var.push(self.expression(&node.value));
        Node::Nodes(var)
    }

    fn conditional_loop(&mut self, node: &nodes::While) -> Node {
        let gid = self.new_group_id();
        let group = self.conditional("while", gid, &node.condition, &node.body);

        Node::Group(gid, group)
    }

    fn unconditional_loop(&mut self, node: &nodes::Loop) -> Node {
        let gid = self.new_group_id();
        let header =
            vec![Node::text("loop"), Node::SpaceOrLine, Node::text("{")];
        let body = self.body(&node.body.values);
        let group = vec![
            self.group(header),
            Node::WrapIf(gid, Box::new(self.group(body))),
        ];

        Node::Group(gid, group)
    }

    fn if_else(&mut self, node: &nodes::If) -> Node {
        let gid = self.new_group_id();
        let mut group = vec![Node::Nodes(self.conditional(
            "if",
            gid,
            &node.if_true.condition,
            &node.if_true.body,
        ))];

        for cond in &node.else_if {
            group.push(Node::Nodes(self.conditional(
                " else if",
                gid,
                &cond.condition,
                &cond.body,
            )));
        }

        if let Some(body) = &node.else_body {
            let body = self.body(&body.values);
            let nodes = vec![
                Node::text(" else {"),
                Node::WrapIf(gid, Box::new(self.group(body))),
            ];

            group.push(Node::Nodes(nodes));
        }

        Node::Group(gid, group)
    }

    fn grouped_expression(&mut self, node: &nodes::Group) -> Node {
        let group = vec![
            Node::text("("),
            Node::Line,
            Node::Indent(vec![self.expression(&node.value)]),
            Node::Line,
            Node::text(")"),
        ];

        self.group(group)
    }

    fn assign_variable(&mut self, node: &nodes::AssignVariable) -> Node {
        Node::Nodes(vec![
            Node::text(&node.variable.name),
            Node::text(" = "),
            self.expression(&node.value),
        ])
    }

    fn replace_variable(&mut self, node: &nodes::ReplaceVariable) -> Node {
        Node::Nodes(vec![
            Node::text(&node.variable.name),
            Node::text(" := "),
            self.expression(&node.value),
        ])
    }

    fn binary_assign_variable(
        &mut self,
        node: &nodes::BinaryAssignVariable,
    ) -> Node {
        let op = Operator::from_ast(node.operator.kind).method_name();

        Node::Nodes(vec![
            Node::text(&node.variable.name),
            Node::text(&format!(" {}= ", op)),
            self.expression(&node.value),
        ])
    }

    fn assign_field(&mut self, node: &nodes::AssignField) -> Node {
        Node::Nodes(vec![
            Node::text(&format!("@{}", node.field.name)),
            Node::text(" = "),
            self.expression(&node.value),
        ])
    }

    fn replace_field(&mut self, node: &nodes::ReplaceField) -> Node {
        Node::Nodes(vec![
            Node::text(&format!("@{}", node.field.name)),
            Node::text(" := "),
            self.expression(&node.value),
        ])
    }

    fn binary_assign_field(&mut self, node: &nodes::BinaryAssignField) -> Node {
        let op = Operator::from_ast(node.operator.kind).method_name();

        Node::Nodes(vec![
            Node::text(&format!("@{}", node.field.name)),
            Node::text(&format!(" {}= ", op)),
            self.expression(&node.value),
        ])
    }

    fn assign_setter(&mut self, node: &nodes::AssignSetter) -> Node {
        Node::Nodes(vec![
            self.expression(&node.receiver),
            Node::text("."),
            Node::text(&node.name.name),
            Node::text(" = "),
            self.expression(&node.value),
        ])
    }

    fn binary_assign_setter(
        &mut self,
        node: &nodes::BinaryAssignSetter,
    ) -> Node {
        let op = Operator::from_ast(node.operator.kind).method_name();

        Node::Nodes(vec![
            self.expression(&node.receiver),
            Node::text("."),
            Node::text(&node.name.name),
            Node::text(&format!(" {}= ", op)),
            self.expression(&node.value),
        ])
    }

    fn reference(&mut self, keyword: &str, node: &Expression) -> Node {
        Node::Nodes(vec![
            Node::text(&format!("{} ", keyword)),
            self.expression(node),
        ])
    }

    fn recover(&mut self, node: &nodes::Recover) -> Node {
        let gid = self.new_group_id();
        let mut header = vec![Node::text("recover ")];
        let body = if node.body.values.len() == 1 {
            let expr = self.expression(&node.body.values[0]);

            Node::IfWrap(
                gid,
                Box::new(Node::Nodes(vec![
                    Node::text("{"),
                    Node::Line,
                    Node::Indent(vec![expr.clone()]),
                    Node::Line,
                    Node::text("}"),
                ])),
                Box::new(expr),
            )
        } else {
            let body = self.body(&node.body.values);

            header.push(Node::text("{"));
            self.group(body)
        };

        Node::Group(gid, vec![self.group(header), body])
    }

    fn throw_value(&mut self, node: &nodes::Throw) -> Node {
        Node::Nodes(vec![Node::text("throw "), self.expression(&node.value)])
    }

    fn try_value(&mut self, node: &nodes::Try) -> Node {
        Node::Nodes(vec![Node::text("try "), self.expression(&node.value)])
    }

    fn return_value(&mut self, node: &nodes::Return) -> Node {
        let mut group = vec![Node::text("return")];

        if let Some(expr) = &node.value {
            group.push(Node::text(" "));
            group.push(self.expression(expr));
        }

        Node::Nodes(group)
    }

    fn tuple(&mut self, node: &nodes::Tuple) -> Node {
        let id = self.new_group_id();

        // Tuples with a length of 1 always need a trailing comma, so we
        // special-case them here as to not complicate the List type more.
        let vals = if node.values.len() == 1 {
            let id = self.new_group_id();
            let expr = self.expression(&node.values[0]);

            Node::Indent(vec![Node::Group(
                id,
                vec![Node::WrapIf(id, Box::new(expr)), Node::text(",")],
            )])
        } else {
            Node::Indent(self.list(&node.values, id, |s, n| s.expression(n)))
        };

        Node::Group(
            id,
            vec![
                Node::text("("),
                Node::Line,
                vals,
                Node::Line,
                Node::text(")"),
            ],
        )
    }

    fn scope(&mut self, node: &nodes::Scope) -> Node {
        let body = self.body(&node.body.values);
        let group = vec![Node::text("{"), self.group(body)];

        Node::Nodes(group)
    }

    fn type_cast(&mut self, node: &nodes::TypeCast) -> Node {
        let group = vec![
            self.expression(&node.value),
            Node::SpaceOrLine,
            Node::Indent(vec![
                Node::text("as "),
                self.type_reference(&node.cast_to),
            ]),
        ];

        self.group(group)
    }

    fn class_literal(&mut self, node: &nodes::ClassLiteral) -> Node {
        let gid = self.new_group_id();
        let group = if node.fields.is_empty() {
            vec![Node::text(&node.class_name.name), Node::text("()")]
        } else if node.fields.len() == 1 {
            let vals = self.list(&node.fields, gid, |this, assign| {
                this.expression(&assign.value)
            });

            vec![
                Node::text(&node.class_name.name),
                Node::text("("),
                Node::Line,
                Node::Indent(vals),
                Node::Line,
                Node::text(")"),
            ]
        } else {
            let vals = self.list(&node.fields, gid, |this, assign| {
                Node::Nodes(vec![
                    Node::text(&format!("{}: ", assign.field.name)),
                    this.expression(&assign.value),
                ])
            });

            vec![
                Node::text(&node.class_name.name),
                Node::text("("),
                Node::Line,
                Node::Indent(vals),
                Node::Line,
                Node::text(")"),
            ]
        };

        Node::Group(gid, group)
    }

    fn call(&mut self, node: &Expression) -> Node {
        let mut calls = Vec::new();
        let mut start = Some(node);

        loop {
            match start {
                Some(Expression::Call(n)) => {
                    start = n.receiver.as_ref();
                    calls.push((&n.name.name, n.arguments.as_ref()));
                }
                Some(node) => {
                    start = Some(node);
                    break;
                }
                _ => break,
            }
        }

        let mut head = Vec::new();
        let mut mid = Vec::new();
        let mut tail = Vec::new();

        if let Some(expr) = start {
            head.push(self.expression(expr));
        }

        let gid = self.new_group_id();

        while let Some((name, node)) = calls.pop() {
            let mut header = if head.is_empty() {
                vec![Node::text(name)]
            } else {
                vec![Node::text("."), Node::text(name)]
            };

            let mut args = Vec::new();

            // When parentheses are explicitly used for expressions such as
            // `User()` and `foo.User()`, we retain the parentheses as they
            // might be used to create an instance of a new class.
            if node.is_some()
                && node.map_or(false, |v| v.values.is_empty())
                && name.chars().next().map_or(false, |v| v.is_uppercase())
            {
                header.push(Node::text("()"));
            } else if let Some(node) = node {
                let list_id = self.new_group_id();
                let mut list = List::new(list_id, node.values.len());
                let max = node.values.len() - 1;

                for (idx, node) in node.values.iter().enumerate() {
                    let arg_id = self.new_group_id();
                    let expr = match node {
                        nodes::Argument::Positional(
                            nodes::Expression::Closure(n),
                        ) => self.closure(n, idx == max),
                        nodes::Argument::Positional(n) => self.expression(n),
                        nodes::Argument::Named(n) => {
                            let pair = vec![
                                Node::text(&n.name.name),
                                Node::text(": "),
                                self.expression(&n.value),
                            ];

                            Node::Nodes(pair)
                        }
                    };

                    list.push(arg_id, expr, true, Node::SpaceOrLine);
                }

                header.push(Node::text("("));

                let group = vec![
                    Node::Line,
                    Node::Indent(list.into_nodes()),
                    Node::Line,
                    Node::text(")"),
                ];

                args.push(Node::Group(list_id, group));
            }

            if head.is_empty() {
                head.push(Node::Nodes(vec![
                    Node::Nodes(header),
                    Node::Nodes(args),
                ]));
            } else if calls.is_empty() {
                mid.push(Node::Nodes(vec![Node::Line, Node::Indent(header)]));

                if !args.is_empty() {
                    tail.push(Node::IfWrap(
                        gid,
                        Box::new(Node::IndentNext(args.clone())),
                        Box::new(Node::Nodes(args)),
                    ));
                }
            } else {
                head.push(Node::Nodes(vec![
                    Node::Line,
                    Node::Indent(vec![Node::Nodes(header), Node::Nodes(args)]),
                ]));
            }
        }

        let tail = if tail.is_empty() {
            None
        } else {
            Some(Box::new(Node::Nodes(tail)))
        };

        Node::Call(
            gid,
            Box::new(Node::Nodes(head)),
            Box::new(Node::Nodes(mid)),
            tail,
        )
    }

    fn closure(&mut self, node: &nodes::Closure, zero_width: bool) -> Node {
        let kw = if node.moving { "fn move" } else { "fn" };
        let mut header = vec![Node::text(kw)];
        let header_id = self.new_group_id();

        if let Some(args) =
            node.arguments.as_ref().filter(|v| !v.values.is_empty())
        {
            let vals = self.list(&args.values, header_id, |this, node| {
                let mut pair = vec![Node::text(&node.name.name)];

                if let Some(typ) = &node.value_type {
                    pair.push(Node::text(": "));
                    pair.push(this.type_reference(typ));
                }

                Node::Nodes(pair)
            });

            header.push(Node::text(" "));
            header.push(Node::Nodes(self.argument_list(vals)));
        }

        if let Some(rnode) = &node.return_type {
            header.push(self.return_type(rnode));
        }

        header.push(Node::text(" {"));

        let body_nodes = self.body(&node.body.values);
        let mut body = self.group(body_nodes);

        if zero_width {
            body = Node::ZeroWidth(Box::new(body));
        }

        self.group(vec![
            Node::Group(header_id, header),
            Node::WrapIf(header_id, Box::new(body)),
        ])
    }

    fn match_value(&mut self, node: &nodes::Match) -> Node {
        let mut cases = Vec::with_capacity(node.expressions.len());
        let mut iter = node.expressions.iter().peekable();

        while let Some(expr) = iter.next() {
            match expr {
                nodes::MatchExpression::Case(n) => {
                    let trailing = match iter.peek() {
                        Some(nodes::MatchExpression::Comment(next))
                            if next.location.is_trailing(&n.location) =>
                        {
                            iter.next();
                            Some(self.comment(next))
                        }
                        _ => None,
                    };

                    cases.push(self.match_case(n));

                    if let Some(node) = trailing {
                        cases.push(Node::text(" "));
                        cases.push(node);
                    }
                }
                nodes::MatchExpression::Comment(n) => {
                    cases.push(self.comment(n));
                }
            }

            if iter.peek().is_some() {
                cases.push(Node::HardLine);
            }
        }

        let expr = self.expression(&node.expression);
        let header = vec![
            Node::text("match"),
            Node::SpaceOrLine,
            Node::Indent(vec![expr]),
            Node::SpaceOrLine,
            Node::text("{"),
        ];
        let group = if cases.is_empty() {
            vec![self.group(header), Node::text("}")]
        } else {
            vec![
                self.group(header),
                Node::HardLine,
                Node::Indent(cases),
                Node::HardLine,
                Node::text("}"),
            ]
        };

        self.group(group)
    }

    fn match_case(&mut self, node: &nodes::MatchCase) -> Node {
        let head_id = self.new_group_id();
        let body_id = self.new_group_id();

        let pat = self.pattern(&node.pattern);
        let pat_id =
            if let Node::Group(v, _) = &pat { *v } else { unreachable!() };
        let mut head = vec![
            Node::text("case"),
            Node::SpaceOrLine,
            Node::Indent(vec![pat]),
        ];

        if let Some(node) = &node.guard {
            let guard = vec![
                Node::IfWrap(
                    pat_id,
                    Box::new(Node::Line),
                    Box::new(Node::SpaceOrLine),
                ),
                Node::text("if "),
                self.expression(node),
            ];

            head.push(self.group(guard))
        }

        let arrow_sep = if matches!(
            node.pattern,
            nodes::Pattern::Or(_)
                | nodes::Pattern::Class(_)
                | nodes::Pattern::Tuple(_)
        ) || node.guard.is_some()
        {
            Node::HardLine
        } else {
            Node::text(" ")
        };
        let arrow = self.group(vec![
            Node::IfWrap(
                head_id,
                Box::new(arrow_sep),
                Box::new(Node::SpaceOrLine),
            ),
            Node::text("-> {"),
        ]);
        let body = if node.body.values.is_empty() {
            vec![arrow, Node::text("}")]
        } else if node.body.values.len() == 1 {
            let expr = self.expression(&node.body.values[0]);
            let wrapped = vec![
                arrow,
                Node::HardLine,
                Node::Indent(vec![expr.clone()]),
                Node::HardLine,
                Node::text("}"),
            ];
            let unwrapped_arrow =
                self.group(vec![Node::SpaceOrLine, Node::text("-> ")]);
            let unwrapped = vec![Node::IfWrap(
                head_id,
                Box::new(self.group(wrapped.clone())),
                Box::new(self.group(vec![unwrapped_arrow, expr])),
            )];

            vec![Node::IfWrap(
                body_id,
                Box::new(self.group(wrapped)),
                Box::new(self.group(unwrapped)),
            )]
        } else {
            vec![
                arrow,
                Node::SpaceOrLine,
                Node::Indent(self.expressions(&node.body.values)),
                Node::SpaceOrLine,
                Node::text("}"),
            ]
        };

        self.group(vec![Node::Group(head_id, head), Node::Group(body_id, body)])
    }

    fn pattern(&mut self, node: &nodes::Pattern) -> Node {
        let group = match node {
            nodes::Pattern::Constant(n) => {
                vec![self.constant(n)]
            }
            nodes::Pattern::Variant(n) => {
                let mut group = vec![Node::text(&n.name.name)];

                if !n.values.is_empty() {
                    let gid = self.new_group_id();
                    let vals = self.list(&n.values, gid, |s, n| s.pattern(n));
                    let args = self.argument_list(vals);

                    group.push(Node::Group(gid, args));
                }

                group
            }
            nodes::Pattern::Class(n) => {
                let gid = self.new_group_id();
                let vals = self.list(&n.values, gid, |this, pat| {
                    let group = vec![
                        Node::text(&format!("@{}", pat.field.name)),
                        Node::text(" = "),
                        this.pattern(&pat.pattern),
                    ];

                    this.group(group)
                });
                let args = vec![
                    Node::text("{"),
                    Node::SpaceOrLine,
                    Node::Indent(vals),
                    Node::SpaceOrLine,
                    Node::text("}"),
                ];

                vec![Node::Group(gid, args)]
            }
            nodes::Pattern::Int(n) => vec![Node::text(&n.value)],
            nodes::Pattern::True(_) => vec![Node::text("true")],
            nodes::Pattern::False(_) => vec![Node::text("false")],
            nodes::Pattern::Identifier(n) => vec![Node::text(&n.name.name)],
            nodes::Pattern::Tuple(n) => {
                let gid = self.new_group_id();
                let vals = self.list(&n.values, gid, |s, n| s.pattern(n));
                let args = self.argument_list(vals);

                vec![Node::Group(gid, args)]
            }
            nodes::Pattern::Wildcard(_) => vec![Node::text("_")],
            nodes::Pattern::Or(n) => {
                let mut iter = n.patterns.iter();
                let head = self.pattern(iter.next().unwrap());
                let mut tail = Vec::new();

                for pat in iter {
                    if !tail.is_empty() {
                        tail.push(Node::SpaceOrLine);
                    }

                    tail.push(Node::text("or "));
                    tail.push(self.pattern(pat));
                }

                vec![head, Node::SpaceOrLine, Node::Indent(tail)]
            }
            nodes::Pattern::String(n) => vec![self.string_literal(n)],
        };

        self.group(group)
    }

    fn type_reference(&mut self, node: &nodes::Type) -> Node {
        match node {
            nodes::Type::Named(n) => self.type_name(n, None),
            nodes::Type::Ref(n) => match &n.type_reference {
                nodes::ReferrableType::Named(n) => {
                    self.type_name(n, Some("ref"))
                }
                nodes::ReferrableType::Closure(n) => {
                    self.closure_type(n, Some("ref"))
                }
                nodes::ReferrableType::Tuple(n) => {
                    self.tuple_type(n, Some("ref"))
                }
            },
            nodes::Type::Mut(n) => match &n.type_reference {
                nodes::ReferrableType::Named(n) => {
                    self.type_name(n, Some("mut"))
                }
                nodes::ReferrableType::Closure(n) => {
                    self.closure_type(n, Some("mut"))
                }
                nodes::ReferrableType::Tuple(n) => {
                    self.tuple_type(n, Some("mut"))
                }
            },
            nodes::Type::Uni(n) => match &n.type_reference {
                nodes::ReferrableType::Named(n) => {
                    self.type_name(n, Some("uni"))
                }
                nodes::ReferrableType::Closure(n) => {
                    self.closure_type(n, Some("uni"))
                }
                nodes::ReferrableType::Tuple(n) => {
                    self.tuple_type(n, Some("uni"))
                }
            },
            nodes::Type::Owned(n) => match &n.type_reference {
                nodes::ReferrableType::Named(n) => {
                    self.type_name(n, Some("move"))
                }
                nodes::ReferrableType::Closure(n) => {
                    self.closure_type(n, Some("move"))
                }
                nodes::ReferrableType::Tuple(n) => {
                    self.tuple_type(n, Some("move"))
                }
            },
            nodes::Type::Closure(n) => self.closure_type(n, None),
            nodes::Type::Tuple(n) => self.tuple_type(n, None),
        }
    }

    fn type_name(
        &mut self,
        node: &nodes::TypeName,
        ownership: Option<&str>,
    ) -> Node {
        let gid = self.new_group_id();
        let name = if let Some(kw) = ownership {
            Node::Nodes(vec![
                Node::text(kw),
                Node::text(" "),
                Node::Nodes(self.constant_name(&node.name)),
            ])
        } else {
            Node::Nodes(self.constant_name(&node.name))
        };

        let nodes = if let Some(args) = &node.arguments {
            let vals = self.list(&args.values, gid, |s, n| s.type_reference(n));

            vec![
                name,
                Node::text("["),
                Node::Line,
                Node::Indent(vals),
                Node::Line,
                Node::text("]"),
            ]
        } else {
            vec![name]
        };

        Node::Group(gid, nodes)
    }

    fn closure_type(
        &mut self,
        node: &nodes::ClosureType,
        ownership: Option<&str>,
    ) -> Node {
        let open = if let Some(kw) = ownership {
            Node::text(&format!("{} fn", kw))
        } else {
            Node::text("fn")
        };
        let mut closure = vec![open];

        if let Some(nodes) =
            node.arguments.as_ref().filter(|v| !v.values.is_empty())
        {
            let gid = self.new_group_id();
            let vals =
                self.list(&nodes.values, gid, |s, n| s.type_reference(n));
            let mut group = vec![
                Node::text(" ("),
                Node::Line,
                Node::Indent(vals),
                Node::Line,
                Node::text(")"),
            ];

            if let Some(rnode) = &node.return_type {
                group.push(self.return_type(rnode));
            }

            closure.push(Node::Group(gid, group));
        } else if let Some(rnode) = &node.return_type {
            closure.push(self.return_type(rnode));
        }

        self.group(closure)
    }

    fn tuple_type(
        &mut self,
        node: &nodes::TupleType,
        ownership: Option<&str>,
    ) -> Node {
        let gid = self.new_group_id();
        let open = if let Some(kw) = ownership {
            Node::text(&format!("{} (", kw))
        } else {
            Node::text("(")
        };

        let vals = self.list(&node.values, gid, |s, n| s.type_reference(n));
        let nodes = vec![
            open,
            Node::Line,
            Node::Indent(vals),
            Node::Line,
            Node::text(")"),
        ];

        Node::Group(gid, nodes)
    }

    fn type_parameter(&mut self, node: &nodes::TypeParameter) -> Node {
        let mut group = vec![Node::text(&node.name.name)];

        if let Some(nodes) = &node.requirements {
            group.push(Node::text(": "));
            group.push(self.type_parameter_requirements(nodes));
        }

        self.group(group)
    }

    fn type_parameter_requirements(
        &mut self,
        nodes: &nodes::Requirements,
    ) -> Node {
        let mut pairs = Vec::new();
        let mut reqs = nodes.values.iter().collect::<Vec<_>>();

        reqs.sort_by(|a, b| match (a, b) {
            (Requirement::Mutable(_), Requirement::Mutable(_)) => {
                Ordering::Equal
            }
            (Requirement::Mutable(_), _) => Ordering::Less,
            (_, Requirement::Mutable(_)) => Ordering::Greater,
            (Requirement::Trait(lhs), Requirement::Trait(rhs)) => {
                lhs.name.name.cmp(&rhs.name.name)
            }
        });

        for (idx, node) in reqs.into_iter().enumerate() {
            let mut pair = Vec::new();

            if idx > 0 {
                pairs.push(Node::SpaceOrLine);
                pair.push(Node::text("+ "));
            }

            let val = match node {
                nodes::Requirement::Trait(n) => self.type_name(n, None),
                nodes::Requirement::Mutable(_) => Node::text("mut"),
            };

            pair.push(val);

            if idx > 0 {
                pairs.push(Node::Indent(pair));
            } else {
                pairs.push(Node::Nodes(pair));
            }
        }

        Node::Nodes(pairs)
    }

    fn new_group_id(&mut self) -> usize {
        let id = self.group_id;

        self.group_id += 1;
        id
    }

    fn list<T, F: FnMut(&mut Document, &T) -> Node>(
        &mut self,
        nodes: &[T],
        group: usize,
        mut func: F,
    ) -> Vec<Node> {
        let mut list = List::new(group, nodes.len());

        for node in nodes {
            list.push(
                self.new_group_id(),
                func(self, node),
                true,
                Node::SpaceOrLine,
            );
        }

        list.into_nodes()
    }

    fn group(&mut self, nodes: Vec<Node>) -> Node {
        Node::Group(self.new_group_id(), nodes)
    }

    fn conditional(
        &mut self,
        keyword: &str,
        group_id: usize,
        condition: &nodes::Expression,
        body: &nodes::Expressions,
    ) -> Vec<Node> {
        let expr = self.expression(condition);
        let header = vec![
            Node::text(keyword),
            Node::SpaceOrLine,
            Node::Indent(vec![expr]),
            Node::SpaceOrLine,
            Node::text("{"),
        ];

        let body = self.body(&body.values);

        vec![
            self.group(header),
            Node::WrapIf(group_id, Box::new(self.group(body))),
        ]
    }
}

#[derive(Copy, Clone, Debug)]
enum Wrap {
    Enable,
    Detect,
    Disable,
    Force,
}

impl Wrap {
    fn is_enabled(self) -> bool {
        matches!(self, Wrap::Enable)
    }

    fn is_force(self) -> bool {
        matches!(self, Wrap::Force)
    }

    fn enable(self) -> Wrap {
        if let Wrap::Disable = self {
            Wrap::Disable
        } else {
            Wrap::Enable
        }
    }

    fn detect(self) -> Wrap {
        match self {
            Wrap::Disable => Wrap::Disable,
            _ => Wrap::Detect,
        }
    }
}

struct Generator {
    buf: String,

    /// The amount of spaces to use for indentation of each line.
    indent: usize,

    /// The number of characters on the current line.
    size: usize,

    /// The group IDs for which wrapping is necessary.
    wrapped: HashSet<usize>,

    pending_indents: usize,
}

impl Generator {
    fn new() -> Generator {
        Generator {
            buf: String::new(),
            indent: 0,
            size: 0,
            wrapped: HashSet::new(),
            pending_indents: 0,
        }
    }

    fn generate(&mut self, node: Node) {
        self.node(node, Wrap::Detect);
    }

    fn node(&mut self, node: Node, wrap: Wrap) {
        match node {
            Node::Nodes(nodes) => {
                for n in nodes {
                    self.node(n, wrap);
                }
            }
            Node::Group(id, nodes) => {
                let width: usize =
                    nodes.iter().map(|n| n.width(&self.wrapped, false)).sum();

                // Groups are wrapped separately, starting with the outer-most
                // group. This way if the group as a whole doesn't fit, we don't
                // immediately wrap _all_ (recursive) child nodes as that could
                // result in unnecessary wrapping.
                let wrap = if let Wrap::Disable = wrap {
                    Wrap::Disable
                } else if wrap.is_force() || (self.size + width) > LIMIT {
                    self.wrapped.insert(id);
                    Wrap::Enable
                } else {
                    Wrap::Detect
                };

                for n in nodes {
                    self.node(n, wrap);
                }
            }
            Node::Fill(nodes) => {
                let mut iter = nodes.into_iter().peekable();
                let mut wrap = wrap;

                while let Some(node) = iter.next() {
                    // If the next node ends beyond the line limit, we need to
                    // insert a newline at the _current_ node.
                    if let Node::SpaceOrLine = node {
                        let width = iter
                            .peek()
                            .map_or(0, |n| n.width(&self.wrapped, false));

                        if self.size + width >= LIMIT {
                            if let Wrap::Detect = wrap {
                                wrap = Wrap::Enable;
                            }

                            self.new_line();
                        } else {
                            self.single_space();
                        }
                    } else {
                        self.node(node, wrap);
                    }
                }
            }
            Node::Call(id, head, mid, tail) => {
                // When calculating the width as a whole we _do_ process
                // child nodes of ZeroWidth nodes, ensuring that long closure
                // bodies properly wrap the call.
                let head_width = head.width(&self.wrapped, true);
                let mid_width = mid.width(&self.wrapped, true);

                if self.size + head_width + mid_width > LIMIT {
                    let wrap = wrap.enable();

                    if wrap.is_enabled() {
                        self.wrapped.insert(id);
                    }

                    self.node(*head, wrap);
                    self.node(*mid, wrap);

                    if let Some(n) = tail {
                        self.node(*n, wrap);
                    }
                } else {
                    let wrap = wrap.detect();

                    self.node(*head, wrap);
                    self.node(*mid, wrap);

                    if let Some(n) = tail {
                        self.node(*n, wrap);
                    }
                }
            }
            Node::Unwrapped(n) => self.node(*n, Wrap::Disable),
            Node::IfWrap(id, n, _) if self.wrapped.contains(&id) => {
                self.node(*n, Wrap::Enable);
            }
            Node::IfWrap(_, _, n) => self.node(*n, wrap),
            Node::WrapIf(id, n) => {
                let wrap =
                    if self.wrapped.contains(&id) { Wrap::Force } else { wrap };

                self.node(*n, wrap);
            }
            Node::Text(v) => {
                self.size += v.len();
                self.buf.push_str(&v);
            }
            Node::Unicode(v, w) => {
                self.size += w;
                self.buf.push_str(&v);
            }
            Node::Line if wrap.is_enabled() => self.new_line(),
            Node::HardLine => self.new_line(),
            Node::EmptyLine => {
                self.buf.push('\n');
                self.new_line();
            }
            Node::SpaceOrLine if wrap.is_enabled() => self.new_line(),
            Node::SpaceOrLine => self.single_space(),
            Node::Indent(nodes) if wrap.is_enabled() => {
                self.size += 2;
                self.indent += 2;
                self.buf.push(INDENT);
                self.buf.push(INDENT);

                for n in nodes {
                    self.node(n, wrap);
                }

                self.indent -= 2;
            }
            Node::Indent(nodes) => {
                for n in nodes {
                    self.node(n, wrap);
                }
            }
            Node::IndentNext(nodes) if wrap.is_enabled() => {
                self.pending_indents += 1;

                for n in nodes {
                    self.node(n, wrap);
                }

                self.indent -= 2;
            }
            Node::IndentNext(nodes) => {
                for n in nodes {
                    self.node(n, wrap);
                }
            }
            Node::ZeroWidth(n) => {
                self.node(*n, wrap);
            }
            _ => {}
        }
    }

    fn single_space(&mut self) {
        self.size += 1;
        self.buf.push(' ');
    }

    fn new_line(&mut self) {
        self.size = self.indent;
        self.buf.push('\n');

        if self.pending_indents > 0 {
            self.size += 2;
            self.indent += 2;
            self.pending_indents -= 1;
        }

        for _ in 0..self.indent {
            self.buf.push(INDENT);
        }
    }
}

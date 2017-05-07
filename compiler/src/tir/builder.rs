//! Functions for converting an AST to TIR.
use std::rc::Rc;
use std::fs::File;
use std::io::Read;
use std::path::MAIN_SEPARATOR;
use std::collections::HashMap;

use compiler::diagnostic::Diagnostic;
use compiler::diagnostics::Diagnostics;
use config::Config;
use parser::{Parser, Node};
use tir::code_object::CodeObject;
use tir::import::Symbol as ImportSymbol;
use tir::instruction::Instruction;
use tir::module::Module;
use tir::registers::Register;
use tir::variable::{Mutability, Scope as VariableScope, Variable};

pub struct Builder {
    pub config: Rc<Config>,

    /// Any diagnostics that were produced when compiling modules.
    pub diagnostics: Diagnostics,

    /// All of the modules that were loaded when compiling a module.
    pub modules: HashMap<String, Module>,
}

impl Builder {
    pub fn new(config: Rc<Config>) -> Self {
        Builder {
            config: config,
            diagnostics: Diagnostics::new(),
            modules: HashMap::new(),
        }
    }

    pub fn build(&mut self, path: String) -> Option<Module> {
        let module = if let Ok(ast) = self.parse_file(&path) {
            let mod_name = self.module_name_for_path(&path);

            let mut module = Module {
                path: path,
                name: mod_name,
                code: CodeObject::new(),
                globals: VariableScope::new(),
            };

            // We need to pass a mutable reference to both the module and the
            // current code object. Rust doesn't allow this because it's usually
            // not safe to do so. Since we know it's safe in this case we're
            // doing this little dance so we don't have to use RefCell instead.
            let code_object =
                unsafe { &mut *(&mut module.code as *mut CodeObject) };

            match ast {
                Node::Expressions { ref nodes } => {
                    self.process_nodes(nodes, &mut module, code_object);
                }
                _ => {}
            };

            Some(module)
        } else {
            None
        };

        module
    }

    fn process_nodes(&mut self,
                     nodes: &Vec<Node>,
                     module: &mut Module,
                     code_object: &mut CodeObject)
                     -> Vec<Register> {
        let mut registers = Vec::new();

        for node in nodes.iter() {
            if let Some(reg) = self.process_node(node, module, code_object) {
                registers.push(reg);
            }
        }

        registers
    }

    fn process_node(&mut self,
                    node: &Node,
                    module: &mut Module,
                    code_object: &mut CodeObject)
                    -> Option<Register> {
        match node {
            &Node::Integer { value, line, column } => {
                Some(self.integer(value, line, column, code_object))
            }
            &Node::Float { value, line, column } => {
                Some(self.float(value, line, column, code_object))
            }
            &Node::String { ref value, line, column } => {
                Some(self.string(value.clone(), line, column, code_object))
            }
            &Node::Array { ref values, line, column } => {
                Some(self.array(values, line, column, module, code_object))
            }
            &Node::Hash { ref pairs, line, column } => {
                Some(self.hash(pairs, line, column, module, code_object))
            }
            &Node::SelfObject { line, column } => {
                Some(self.get_self(line, column, code_object))
            }
            &Node::Identifier { ref name, line, column } => {
                Some(self.identifier(name, line, column, module, code_object))
            }
            &Node::Attribute { ref name, line, column } => {
                Some(self.attribute(name.clone(), line, column, code_object))
            }
            &Node::Constant { ref name, line, column } => {
                Some(self.get_constant(name.clone(), line, column, code_object))
            }
            &Node::Path { ref steps } => self.path(steps, module, code_object),
            &Node::ConstDefine { ref name, ref value, line, column } => {
                Some(self.set_constant(name.clone(),
                                       value,
                                       line,
                                       column,
                                       module,
                                       code_object))
            }
            &Node::LetDefine { ref name, ref value, line, column } => {
                self.set_variable(name,
                                  value,
                                  Mutability::Immutable,
                                  line,
                                  column,
                                  module,
                                  code_object)
            }
            &Node::VarDefine { ref name, ref value, line, column } => {
                self.set_variable(name,
                                  value,
                                  Mutability::Mutable,
                                  line,
                                  column,
                                  module,
                                  code_object)
            }
            &Node::Send { ref name,
                          ref receiver,
                          ref arguments,
                          line,
                          column } => {
                Some(self.send_object_message(name.clone(),
                                              receiver,
                                              arguments,
                                              line,
                                              column,
                                              module,
                                              code_object))
            }
            &Node::Import { ref steps, ref symbols, line, column } => {
                Some(self.import(steps, symbols, line, column, code_object))
            }
            _ => None,
        }
    }

    fn integer(&self,
               val: i64,
               line: usize,
               col: usize,
               code: &mut CodeObject)
               -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::SetInteger {
            register: register,
            value: val,
            line: line,
            column: col,
        });

        register
    }

    fn float(&self,
             val: f64,
             line: usize,
             col: usize,
             code: &mut CodeObject)
             -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::SetFloat {
            register: register,
            value: val,
            line: line,
            column: col,
        });

        register
    }

    fn string(&self,
              val: String,
              line: usize,
              col: usize,
              code: &mut CodeObject)
              -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::SetString {
            register: register,
            value: val,
            line: line,
            column: col,
        });

        register
    }

    fn array(&mut self,
             value_nodes: &Vec<Node>,
             line: usize,
             col: usize,
             module: &mut Module,
             code: &mut CodeObject)
             -> Register {
        let register = code.registers.reserve();
        let values = self.process_nodes(&value_nodes, module, code);

        code.instructions.push(Instruction::SetArray {
            register: register,
            values: values,
            line: line,
            column: col,
        });

        register
    }

    fn hash(&mut self,
            pair_nodes: &Vec<(Node, Node)>,
            line: usize,
            col: usize,
            module: &mut Module,
            code: &mut CodeObject)
            -> Register {
        let register = code.registers.reserve();
        let mut pairs = Vec::new();

        for &(ref k, ref v) in pair_nodes.iter() {
            if let Some(key_reg) = self.process_node(k, module, code) {
                if let Some(val_reg) = self.process_node(v, module, code) {
                    pairs.push((key_reg, val_reg));
                }
            }
        }

        code.instructions.push(Instruction::SetHash {
            register: register,
            pairs: pairs,
            line: line,
            column: col,
        });

        register
    }

    fn get_self(&self,
                line: usize,
                col: usize,
                code: &mut CodeObject)
                -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::GetSelf {
            register: register,
            line: line,
            column: col,
        });

        register
    }

    fn identifier(&mut self,
                  name: &String,
                  line: usize,
                  col: usize,
                  module: &mut Module,
                  code: &mut CodeObject)
                  -> Register {
        // TODO: look up methods before looking up globals
        if let Some(local) = code.variables.lookup(name) {
            self.get_local(local, line, col, code)
        } else if let Some(global) = module.globals.lookup(name) {
            self.get_global(global, line, col, code)
        } else {
            // TODO: check if the method actually exists.
            let args = Vec::new();

            self.send_object_message(name.clone(),
                                     &None,
                                     &args,
                                     line,
                                     col,
                                     module,
                                     code)
        }
    }

    fn attribute(&mut self,
                 name: String,
                 line: usize,
                 col: usize,
                 code: &mut CodeObject)
                 -> Register {
        let self_reg = self.get_self(line, col, code);
        let register = code.registers.reserve();

        code.instructions.push(Instruction::GetAttribute {
            register: register,
            receiver: self_reg,
            name: name,
            line: line,
            column: col,
        });

        register
    }

    fn get_local(&mut self,
                 variable: Variable,
                 line: usize,
                 col: usize,
                 code: &mut CodeObject)
                 -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::GetLocal {
            register: register,
            variable: variable,
            line: line,
            column: col,
        });

        register
    }

    fn get_global(&mut self,
                  variable: Variable,
                  line: usize,
                  col: usize,
                  code: &mut CodeObject)
                  -> Register {
        let register = code.registers.reserve();

        code.instructions.push(Instruction::GetGlobal {
            register: register,
            variable: variable,
            line: line,
            column: col,
        });

        register
    }

    fn get_constant(&mut self,
                    name: String,
                    line: usize,
                    col: usize,
                    code: &mut CodeObject)
                    -> Register {
        let self_reg = self.get_self(line, col, code);
        let register = code.registers.reserve();

        code.instructions.push(Instruction::GetConstant {
            receiver: self_reg,
            register: register,
            name: name,
            line: line,
            column: col,
        });

        register
    }

    fn path(&mut self,
            steps: &Vec<Node>,
            module: &mut Module,
            code: &mut CodeObject)
            -> Option<Register> {
        let mut iter = steps.iter();

        // a path always has 1 element, so we can safely call unwrap here.
        let mut register = match iter.next().unwrap() {
            &Node::Identifier { ref name, line, column } => {
                self.identifier(name, line, column, module, code)
            }
            &Node::Constant { ref name, line, column } => {
                self.get_constant(name.clone(), line, column, code)
            }
            // TODO: handle Node::Type
            // Because of the parser this arm will never be reached.
            _ => unreachable!(),
        };

        while let Some(step) = iter.next() {
            match step {
                &Node::Identifier { ref name, line, column } |
                &Node::Constant { ref name, line, column } => {
                    let step_register = code.registers.reserve();

                    code.instructions.push(Instruction::GetConstant {
                        receiver: register,
                        register: step_register,
                        name: name.clone(),
                        line: line,
                        column: column,
                    });

                    register = step_register;
                }
                _ => {}
            }
        }

        Some(register)
    }

    fn set_constant(&mut self,
                    name: String,
                    value_node: &Node,
                    line: usize,
                    col: usize,
                    module: &mut Module,
                    code: &mut CodeObject)
                    -> Register {
        let self_reg = self.get_self(line, col, code);
        let register = code.registers.reserve();
        let value_reg = self.process_node(&value_node, module, code).unwrap();

        code.instructions.push(Instruction::SetConstant {
            receiver: self_reg,
            register: register,
            name: name,
            value: value_reg,
            line: line,
            column: col,
        });

        register
    }

    fn set_variable(&mut self,
                    name_node: &Node,
                    value_node: &Node,
                    mutability: Mutability,
                    line: usize,
                    column: usize,
                    module: &mut Module,
                    code: &mut CodeObject)
                    -> Option<Register> {
        let value_reg = self.process_node(value_node, module, code).unwrap();

        match name_node {
            &Node::Identifier { ref name, .. } => {
                Some(self.set_local(name.clone(),
                                    value_reg,
                                    mutability,
                                    line,
                                    column,
                                    code))
            }
            &Node::Attribute { ref name, .. } => {
                Some(self.set_attribute(name.clone(),
                                        value_reg,
                                        line,
                                        column,
                                        code))
            }
            _ => None,
        }
    }

    fn set_local(&mut self,
                 name: String,
                 value: Register,
                 mutability: Mutability,
                 line: usize,
                 col: usize,
                 code: &mut CodeObject)
                 -> Register {
        let register = code.registers.reserve();
        let variable = code.variables.define(name, mutability);

        code.instructions.push(Instruction::SetLocal {
            register: register,
            variable: variable,
            value: value,
            line: line,
            column: col,
        });

        register
    }

    fn set_attribute(&self,
                     name: String,
                     value: Register,
                     line: usize,
                     col: usize,
                     code: &mut CodeObject)
                     -> Register {
        let register = code.registers.reserve();
        let self_reg = self.get_self(line, col, code);

        // TODO: track mutability of attributes per receiver type

        code.instructions.push(Instruction::SetAttribute {
            register: register,
            receiver: self_reg,
            name: name,
            value: value,
            line: line,
            column: col,
        });

        register
    }

    fn send_object_message(&mut self,
                           name: String,
                           receiver_node: &Option<Box<Node>>,
                           arguments: &Vec<Node>,
                           line: usize,
                           col: usize,
                           module: &mut Module,
                           code: &mut CodeObject)
                           -> Register {
        let register = code.registers.reserve();

        let receiver_reg = if let &Some(ref rec) = receiver_node {
            self.process_node(rec, module, code).unwrap()
        } else {
            self.get_self(line, col, code)
        };

        let mut arg_regs = vec![receiver_reg];

        for arg in arguments.iter() {
            arg_regs.push(self.process_node(arg, module, code).unwrap());
        }

        code.instructions.push(Instruction::SendObjectMessage {
            register: register,
            receiver: receiver_reg,
            name: name,
            arguments: arg_regs,
            line: line,
            column: col,
        });

        register
    }

    /// Converts the list of import steps to a module name.
    fn module_name_for_import(&self, steps: &Vec<Node>) -> String {
        let mut chunks = Vec::new();

        for step in steps.iter() {
            match step {
                &Node::Identifier { ref name, .. } => {
                    chunks.push(name.clone());
                }
                &Node::Constant { .. } => break,
                _ => {}
            }
        }

        chunks.join(self.config.lookup_separator())
    }

    /// Returns a vector of symbols to import, based on a list of AST nodes
    /// describing the import steps.
    fn import_symbols(&self, nodes: &Vec<Node>) -> Vec<ImportSymbol> {
        let mut symbols = Vec::new();

        for node in nodes.iter() {
            match node {
                &Node::ImportSymbol { symbol: ref symbol_node,
                                      alias: ref alias_node } => {
                    let alias = if let &Some(ref node) = alias_node {
                        self.name_of_node(node)
                    } else {
                        None
                    };

                    let symbol = match **symbol_node {
                        Node::Identifier { ref name, line, column } => {
                            ImportSymbol::identifier(name.clone(),
                                                     alias,
                                                     line,
                                                     column)
                        }
                        Node::Constant { ref name, line, column } => {
                            ImportSymbol::constant(name.clone(),
                                                   alias,
                                                   line,
                                                   column)
                        }
                        _ => continue,
                    };

                    symbols.push(symbol);
                }
                _ => {}
            }
        }

        symbols
    }

    fn import(&mut self,
              step_nodes: &Vec<Node>,
              symbol_nodes: &Vec<Node>,
              line: usize,
              col: usize,
              code: &mut CodeObject)
              -> Register {
        let register = code.registers.reserve();
        let mod_name = self.module_name_for_import(step_nodes);
        let mod_path = self.module_path_for_name(&mod_name);

        let full_path = match self.find_module_path(&mod_path) {
            Some(path) => path,
            None => return register,
        };

        if let Some(module) = self.build(full_path) {
            self.modules.insert(mod_name, module);
        }

        for symbol in self.import_symbols(symbol_nodes) {
            match symbol {
                ImportSymbol::Identifier { ref name, ref alias, .. } => {
                    // TODO: look up the symbols and define them as globals
                }
                ImportSymbol::Constant { ref name, ref alias, .. } => {
                    // TODO: look up the symbols and define them as globals
                }
            }
        }

        register
    }

    fn name_of_node(&self, node: &Node) -> Option<String> {
        match node {
            &Node::Identifier { ref name, .. } |
            &Node::Constant { ref name, .. } => Some(name.clone()),
            _ => None,
        }
    }

    fn parse_file(&mut self, path: &String) -> Result<Node, ()> {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(err) => {
                self.diagnostics.error(path, err.to_string(), 1, 1);
                return Err(());
            }
        };

        let mut input = String::new();

        if let Err(err) = file.read_to_string(&mut input) {
            self.diagnostics.error(path, err.to_string(), 1, 1);
            return Err(());
        }

        let mut parser = Parser::new(&input);

        match parser.parse() {
            Ok(ast) => Ok(ast),
            Err(err) => {
                self.diagnostics.error(path, err, parser.line(), parser.column());

                Err(())
            }
        }
    }

    fn module_name_for_path(&self, path: &String) -> String {
        if let Some(file_with_ext) = path.split(MAIN_SEPARATOR).last() {
            if let Some(file_name) = file_with_ext.split(".").next() {
                return file_name.to_string();
            }
        }

        String::new()
    }

    fn module_path_for_name(&self, name: &str) -> String {
        let file_name =
            name.replace(self.config.lookup_separator(),
                         &MAIN_SEPARATOR.to_string());

        file_name + self.config.source_extension()
    }

    fn find_module_path(&self, path: &str) -> Option<String> {
        for dir in self.config.source_directories.iter() {
            let full_path = dir.join(path);

            if full_path.exists() {
                return Some(full_path.to_str().unwrap().to_string());
            }
        }

        None
    }
}

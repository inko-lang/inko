//! Pretty-printing of MIR for debugging purposes.
//!
//! This module currently allows unused functions, as it's not yet clear how
//! we're going to expose this in the compiler. See
//! https://gitlab.com/inko-lang/inko/-/issues/251 for more information.
#![allow(unused)]

use crate::mir::{BlockId, Method, Mir};
use std::fmt::Write;
use types::{Database, TypeId};

/// Returns a String containing Dot/graphviz code for visualising the MIR of one
/// or more methods.
pub(crate) fn to_dot(db: &Database, mir: &Mir, methods: &[&Method]) -> String {
    let mut buffer = String::new();

    buffer.push_str("digraph MIR {\n");

    for (method_index, method) in methods.iter().enumerate() {
        let _ = writeln!(buffer, "subgraph cluster_MIR_{} {{", method_index);

        buffer.push_str("graph[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("node[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("edge[fontname=\"monospace\", fontsize=10];\n");

        let rec_name = match method.id.self_type(db) {
            TypeId::Class(id) => id.name(db),
            TypeId::Trait(id) => id.name(db),
            TypeId::ClassInstance(ins) => ins.instance_of().name(db),
            TypeId::TraitInstance(ins) => ins.instance_of().name(db),
            _ => "",
        };

        let name = if rec_name.is_empty() {
            format!("{}()", method.id.name(db))
        } else {
            format!("{}::{}()", rec_name, method.id.name(db))
        };

        let _ = writeln!(buffer, "label=<{}>;", name);

        let reachable_blocks = method.body.reachable();

        for (index, block) in method.body.blocks.iter().enumerate() {
            let reachable = reachable_blocks.contains(&BlockId(index));
            let _ = write!(
                buffer,
                "  b{}{}[shape=\"none\" label=\
                <<table border=\"0\" cellborder=\"1\" cellspacing=\"0\">",
                method_index, index
            );

            let _ = write!(
                buffer,
                "<tr>\
                <td colspan=\"2\" bgcolor=\"{}\" align=\"center\">\
                b{}{}\
                </td>\
                </tr>",
                if reachable { "CornSilk" } else { "LightGray" },
                index,
                if reachable { "" } else { " (unreachable)" }
            );

            buffer.push_str(
                "<tr>\
                <td bgcolor=\"#f2f2f2\">Instruction</td>\
                <td bgcolor=\"#f2f2f2\">Line</td>\
                </tr>",
            );

            for ins in &block.instructions {
                let _ = write!(
                    buffer,
                    "<tr><td>{}</td><td>{}</td></tr>",
                    ins.format(db).replace('>', "&gt;").replace('<', "&lt;"),
                    mir.location(ins.location()).range.line_range.start(),
                );
            }

            buffer.push_str("</table>>];\n");
        }

        for index in 0..method.body.blocks.len() {
            for edge in method.body.successors(BlockId(index)) {
                let _ = writeln!(
                    buffer,
                    "  b{}{} -> b{}{}",
                    method_index, index, method_index, edge.0
                );
            }
        }

        buffer.push_str("}\n");
    }

    buffer.push_str("}\n");
    buffer
}

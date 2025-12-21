//! Pretty-printing of MIR for debugging purposes.
use crate::mir::inline::method_weight;
use crate::mir::{BlockId, Method};
use crate::symbol_names::SymbolNames;
use std::fmt::Write;
use types::Database;

fn html_escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('>', "&gt;").replace('<', "&lt;")
}

/// Returns a String containing Dot/graphviz code for visualising the MIR of one
/// or more methods.
pub(crate) fn to_dot(
    db: &Database,
    names: &SymbolNames,
    methods: &[&Method],
) -> String {
    let mut buffer = String::new();

    buffer.push_str("digraph MIR {\n");

    for (method_index, &method) in methods.iter().enumerate() {
        let _ = writeln!(buffer, "subgraph cluster_MIR_{} {{", method_index);

        buffer.push_str("graph[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("node[fontname=\"monospace\", fontsize=10];\n");
        buffer.push_str("edge[fontname=\"monospace\", fontsize=10];\n");

        let name = html_escape(&names.methods[&method.id]);
        let _ = writeln!(
            buffer,
            "label=\"{}\nroot = b{}, inline weight = {}\";",
            name,
            method.body.start_id.0,
            method_weight(db, method),
        );
        let reachable_blocks = method.body.reachable();

        // Render a hidden node that points to the entry block, ensuring the
        // entry block is always placed at the top of the graph.
        let _ = writeln!(
            buffer,
            "  root{}[style=invis,height=0,wight=0,margin=0]",
            method_index
        );
        let _ = writeln!(
            buffer,
            "  root{} -> b{}{} [style=invis,constraint=false]",
            method_index, method_index, method.body.start_id.0
        );

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
                    html_escape(&ins.format(db, names)),
                    ins.location().line,
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

pub(crate) fn to_text(
    db: &Database,
    names: &SymbolNames,
    methods: &[&Method],
) -> String {
    let mut buffer = String::new();

    for method in methods {
        let reachable_blocks = method.body.reachable();
        let mut num_blk = 0;
        let mut num_ins = 0;

        for (idx, blk) in method.body.blocks.iter().enumerate() {
            if !reachable_blocks.contains(&BlockId(idx)) {
                continue;
            }

            num_blk += 1;
            num_ins += blk.instructions.len();
        }

        let name = &names.methods[&method.id];
        let start = BlockId(method.body.start_id.0);
        let _ = writeln!(
            buffer,
            "\
--------------------------------------------------------------------------------
{}
ID = {}, line = {}
start = b{}, blocks = {}, instructions = {}, inline weight = {}
--------------------------------------------------------------------------------",
            name,
            method.id.0,
            method.id.location(db).line_start,
            start.0,
            num_blk,
            num_ins,
            method_weight(db, method),
        );

        method.body.each_block_in_order(|id| {
            let blk = &method.body.block(id);
            let mut header = format!("b{}:", id.0);

            if blk.cold {
                header = format!("{:<50} # cold", header);
            }

            buffer.push_str(&header);
            buffer.push('\n');

            for ins in &blk.instructions {
                let _ = writeln!(buffer, "  {}", ins.format(db, names));
            }
        });

        buffer.push('\n');
    }

    buffer
}

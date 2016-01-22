use std::env;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    Command::new("ragel")
        .args(&["-U", "src/lexer.rl", "-o"])
        .arg(&format!("{}/lexer.rs", out_dir))
        .status()
        .unwrap();

    println!("cargo:rerun-if-changed=src/lexer.rl");
}

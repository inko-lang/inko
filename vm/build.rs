fn main() {
    if !cfg!(target_arch = "x86_64") {
        panic!("The Inko virtual machine requires a 64-bits architecture");
    }
}

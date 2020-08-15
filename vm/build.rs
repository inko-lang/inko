fn main() {
    if !cfg!(target_pointer_width = "64") {
        panic!("The Inko virtual machine requires a 64-bits architecture");
    }
}

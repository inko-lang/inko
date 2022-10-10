fn main() {
    if !cfg!(target_pointer_width = "64") {
        panic!("Inko requires a 64-bits architecture");
    }

    if !cfg!(target_endian = "little") {
        panic!("Inko requires a little-endian architecture");
    }
}

#![macro_use]

/// Returns a string to use for reading from a file, optionally with a max size.
macro_rules! file_reading_buffer {
    ($instruction: ident, $process: ident, $size_idx: expr) => (
        if $instruction.arguments.get($size_idx).is_some() {
            let size_ptr = instruction_object!($instruction, $process,
                                               $size_idx);

            let size_ref = size_ptr.get();
            let size_obj = size_ref.get();

            ensure_integers!(size_obj);

            let size = size_obj.value.as_integer();

            ensure_positive_read_size!(size);

            String::with_capacity(size as usize)
        }
        else {
            String::new()
        }
    );
}

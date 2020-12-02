use crate::closable::ClosableFile;
use crate::immix::copy_object::CopyObject;
use crate::object_pointer::ObjectPointer;
use crate::runtime_error::RuntimeError;
use std::fs;
use std::fs::OpenOptions;

/// File opened for reading, equal to fopen's "r" mode.
pub const READ: i64 = 0;

/// File opened for writing, equal to fopen's "w" mode.
pub const WRITE: i64 = 1;

/// File opened for appending, equal to fopen's "a" mode.
pub const APPEND: i64 = 2;

/// File opened for both reading and writing, equal to fopen's "w+" mode.
pub const READ_WRITE: i64 = 3;

/// File opened for reading and appending, equal to fopen's "a+" mode.
pub const READ_APPEND: i64 = 4;

/// A file and its path.
pub struct File {
    /// The raw file.
    inner: ClosableFile,

    /// The path used to open the file.
    path: ObjectPointer,
}

impl File {
    pub fn open(path: ObjectPointer, mode: i64) -> Result<File, RuntimeError> {
        let mut open_opts = OpenOptions::new();

        match mode {
            READ => open_opts.read(true),
            WRITE => open_opts.write(true).truncate(true).create(true),
            APPEND => open_opts.append(true).create(true),
            READ_WRITE => open_opts.read(true).write(true).create(true),
            READ_APPEND => open_opts.read(true).append(true).create(true),
            _ => return Err(format!("Invalid file open mode: {}", mode).into()),
        };

        let file = open_opts.open(path.string_value()?)?;

        Ok(File {
            inner: ClosableFile::new(file),
            path,
        })
    }

    pub fn path(&self) -> &ObjectPointer {
        &self.path
    }

    pub fn get_mut(&mut self) -> &mut fs::File {
        &mut self.inner
    }

    pub fn close(&mut self) {
        self.inner.close();
    }

    pub fn clone_to<H: CopyObject>(
        &self,
        heap: &mut H,
    ) -> Result<Self, RuntimeError> {
        Ok(File {
            inner: self.inner.try_clone()?,
            path: heap.copy_object(self.path)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn test_memory_size() {
        assert_eq!(size_of::<File>(), 16);
    }
}

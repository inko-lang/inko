use std::env;
use std::fs::{remove_file, File};
use std::io::Write;
use std::process;
use std::string::ToString;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Tempfile {
    path: String,
    file: File,
}

impl Tempfile {
    pub fn new(extension: &str) -> Result<Self, String> {
        // This is a poor man's way of generating a somewhat unique temporary
        // file name. The alternative is using the tempfile crate, but this is
        // overkill for what we need.
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                "Failed to generate a temporary file path".to_string()
            })?
            .as_secs();

        let mut tmp_path = env::temp_dir();

        tmp_path.push(format!(
            "inko-tempfile-{}-{}.{}",
            process::id(),
            time,
            extension
        ));

        let path = tmp_path.to_string_lossy().to_string();
        let file = File::create(&path).map_err(|e| e.to_string())?;

        Ok(Tempfile { path, file })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.file.write_all(bytes).map_err(|e| e.to_string())
    }

    pub fn flush(&mut self) {
        let _failure_doesnt_matter = self.file.flush();
    }

    pub fn path(&self) -> &String {
        &self.path
    }
}

impl Drop for Tempfile {
    fn drop(&mut self) {
        let _failure_doesnt_matter = remove_file(&self.path);
    }
}

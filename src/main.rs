use clap::Parser;
use cli::Args;
use config::AggConfig;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tiktoken_rs::cl100k_base;

mod cli;
mod config;

struct CountingWriter<W: Write> {
    inner: W,
    bytes_written: usize,
    content: String,
}

impl<W: Write> CountingWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            bytes_written: 0,
            content: String::new(),
        }
    }

    fn bytes_written(&self) -> usize {
        self.bytes_written
    }

    fn content(&self) -> &str {
        &self.content
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes_written += n;
        if let Ok(s) = std::str::from_utf8(&buf[..n]) {
            self.content.push_str(s);
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn main() -> io::Result<()> {
    let args = AggConfig::load()
        .map(Args::from)
        .unwrap_or_else(Args::parse);

    let inner_writer: Box<dyn Write> = match args.output {
        Some(ref path) => Box::new(BufWriter::new(File::create(path).unwrap())),
        None => Box::new(BufWriter::new(io::stdout())),
    };
    let mut writer = CountingWriter::new(inner_writer);

    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    let gitignore = load_ignore_file(&root, ".gitignore");
    let aggignore = load_ignore_file(&root, ".aggignore");

    visit_dirs(
        &root,
        &mut writer,
        &args.allowed_extensions,
        args.include_binary,
        &args.exclude_dirs,
        &gitignore,
        &aggignore,
    )?;

    writer.flush()?;

    // Count tokens using cl100k_base (used by GPT-4, Claude, etc.)
    let bpe = cl100k_base().unwrap();
    let tokens = bpe.encode_with_special_tokens(writer.content());
    let token_count = tokens.len();
    let bytes = writer.bytes_written();

    eprintln!();
    eprintln!("Output: {} bytes, ~{} tokens", bytes, token_count);

    Ok(())
}

fn load_ignore_file(root: &Path, filename: &str) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let ignore_path = root.join(filename);
    if ignore_path.exists() {
        match builder.add(ignore_path) {
            None => (),
            Some(err) => eprintln!("Error adding {}: {}", filename, err),
        }
    }
    builder.build().unwrap_or_else(|err| {
        eprintln!("Error building {}: {}", filename, err);
        Gitignore::empty()
    })
}

fn visit_dirs(
    dir: &PathBuf,
    writer: &mut impl Write,
    allowed_extensions: &[String],
    include_binary: bool,
    exclude_dirs: &[String],
    gitignore: &Gitignore,
    aggignore: &Gitignore,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Check if the directory should be excluded
            let current_dir = path.file_name().unwrap().to_str().unwrap();
            if exclude_dirs.contains(&current_dir.to_string()) {
                continue;
            }

            // Check both gitignore and aggignore patterns
            if gitignore.matched(&path, path.is_dir()).is_ignore()
                || aggignore.matched(&path, path.is_dir()).is_ignore()
            {
                continue; // Skip ignored files/directories
            }

            if path.is_dir() {
                visit_dirs(
                    &path,
                    writer,
                    allowed_extensions,
                    include_binary,
                    exclude_dirs,
                    gitignore,
                    aggignore,
                )?;
            } else if should_process_file(&path, allowed_extensions) {
                match process_file(&path, writer, include_binary) {
                    Ok(_) => (),
                    Err(e) => eprintln!("Error processing file {:?}: {}", path, e),
                }
            }
        }
    }
    Ok(())
}

fn should_process_file(file_path: &Path, allowed_extensions: &[String]) -> bool {
    if allowed_extensions.is_empty() {
        return true; // Process all files if no extensions are specified
    }
    if let Some(extension) = file_path.extension() {
        let ext = extension.to_str().unwrap_or("").to_lowercase();
        allowed_extensions.contains(&ext)
    } else {
        false
    }
}

fn process_file(
    file_path: &PathBuf,
    writer: &mut impl Write,
    include_binary: bool,
) -> io::Result<()> {
    let mut file = File::open(file_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    // Check if the file is UTF-8 encoded
    if let Ok(contents) = String::from_utf8(buffer.clone()) {
        write_file_contents(file_path, writer, &contents)
    } else if include_binary {
        // If not UTF-8 and binary files are allowed, encode as base64
        #[allow(deprecated)]
        let base64 = base64::encode(&buffer);
        write_file_contents(
            file_path,
            writer,
            &format!("[Binary data encoded as base64]:\n{}", base64),
        )
    } else {
        // eprintln!("Skipping non-UTF8 file: {:?}", file_path);
        Ok(())
    }
}

fn write_file_contents(
    file_path: &Path,
    writer: &mut impl Write,
    contents: &str,
) -> io::Result<()> {
    let start_marker = format!("<<<START_FILE:{}>>\n", file_path.display());
    let end_marker = format!("<<<END_FILE:{}>>\n", file_path.display());
    writer.write_all(start_marker.as_bytes())?;
    writer.write_all(contents.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.write_all(end_marker.as_bytes())?;
    Ok(())
}

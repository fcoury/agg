use clap::Parser;
use cli::Args;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

mod cli;

fn main() -> io::Result<()> {
    let args = Args::parse();
    let mut writer: Box<dyn Write> = match args.output {
        Some(ref path) => Box::new(BufWriter::new(File::create(path).unwrap())),
        None => Box::new(BufWriter::new(io::stdout())),
    };

    let root = args.path.unwrap_or_else(|| PathBuf::from("."));
    let gitignore = load_gitignore(&root);

    visit_dirs(
        &root,
        &mut writer,
        &args.allowed_extensions,
        args.include_binary,
        &gitignore,
    )?;

    Ok(())
}

fn load_gitignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let gitignore_path = root.join(".gitignore");
    eprintln!("Reading .gitignore from {:?}", gitignore_path);
    if gitignore_path.exists() {
        _ = builder.add(gitignore_path);
    }
    builder.build().unwrap_or_else(|err| {
        eprintln!("Error building gitignore: {}", err);
        Gitignore::empty()
    })
}

fn visit_dirs(
    dir: &PathBuf,
    writer: &mut Box<dyn Write>,
    allowed_extensions: &[String],
    include_binary: bool,
    gitignore: &Gitignore,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if gitignore.matched(&path, path.is_dir()).is_ignore() {
                continue; // Skip ignored files/directories
            }
            if path.is_dir() {
                visit_dirs(&path, writer, allowed_extensions, include_binary, gitignore)?;
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

fn should_process_file(file_path: &PathBuf, allowed_extensions: &[String]) -> bool {
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
    writer: &mut Box<dyn Write>,
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
    file_path: &PathBuf,
    writer: &mut Box<dyn Write>,
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

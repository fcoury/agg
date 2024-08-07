use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut allowed_extensions = Vec::new();
    let mut include_binary = false;
    let mut start_path = PathBuf::from(".");

    // Parse command line arguments
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--include-binary" => include_binary = true,
            "--path" => {
                if i + 1 < args.len() {
                    start_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                } else {
                    eprintln!("Error: --path option requires a directory path");
                    std::process::exit(1);
                }
            }
            arg if !arg.starts_with("--") => allowed_extensions.push(arg.to_lowercase()),
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("Start path: {:?}", start_path);
    println!("Allowed extensions: {:?}", allowed_extensions);
    println!("Include binary files: {}", include_binary);

    let output_file = File::create("concatenated_output.txt")?;
    let mut writer = io::BufWriter::new(output_file);

    visit_dirs(
        &start_path,
        &mut writer,
        &allowed_extensions,
        include_binary,
    )?;

    Ok(())
}

fn visit_dirs(
    dir: &PathBuf,
    writer: &mut io::BufWriter<File>,
    allowed_extensions: &[String],
    include_binary: bool,
) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, writer, allowed_extensions, include_binary)?;
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
    writer: &mut io::BufWriter<File>,
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
        eprintln!("Skipping non-UTF8 file: {:?}", file_path);
        Ok(())
    }
}

fn write_file_contents(
    file_path: &PathBuf,
    writer: &mut io::BufWriter<File>,
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

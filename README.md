# agg

A command-line utility for preparing code snippets for AI tools by aggregating source code files from a project into a single text file.

## Features

- Recursively traverses directories to collect source files
- Supports filtering by file extensions
- Handles binary files (optional base64 encoding)
- Respects `.gitignore` and `.aggignore` patterns
- Configurable through command line arguments or `.aggconfig` file
- Excludes specified directories
- Outputs to file or stdout

## Installation

Make sure you have Rust installed, then:

```bash
git clone [repository-url]
cd agg
cargo install --path .
```

## Usage

### Command Line

Basic usage:

```bash
# Output all files in current directory to stdout
agg

# Specify source directory and output file
agg -p /path/to/source -o output.txt

# Only include specific file extensions
agg rs toml md

# Include binary files (base64 encoded)
agg -b

# Exclude specific directories
agg -e node_modules -e target
```

### Configuration File

Instead of command line arguments, you can create a `.aggconfig` file in TOML format:

```toml
include_binary = true
path = "./src"
output = "output.txt"
exclude_dirs = ["target", "node_modules"]
allowed_extensions = ["rs", "toml"]
```

The program will automatically detect and use the `.aggconfig` file if present. Command line arguments take precedence if both are provided.

## Command Line Options

```
Options:
  -b, --include-binary     Include binary files as base64 encoded strings
  -p, --path <PATH>        Initial path to start searching [default: current directory]
  -o, --output <OUTPUT>    Output file [default: stdout]
  -e, --exclude-dirs <DIRS>    Directories to exclude
  [EXTENSIONS]             File extensions to include (space-separated)
```

## Ignore Files

### .gitignore

The tool respects existing `.gitignore` files in your project directory.

### .aggignore

Sometimes you want to ignore files when generating text for AI but you don't want to prevent the file from being added to git.

You can create an `.aggignore` file specifically for this case. It follows the same pattern syntax as `.gitignore`.

## Example Output

The tool generates output in the following format:

```
<<<START_FILE:./src/main.rs>>
// File contents here
<<<END_FILE:./src/main.rs>>
<<<START_FILE:./src/lib.rs>>
// File contents here
<<<END_FILE:./src/lib.rs>>
```

## Error Handling

- Non-UTF8 files are skipped unless `-b` flag is used
- Binary files are base64 encoded when included
- Invalid configuration files or command line arguments produce error messages
- File access errors are reported but don't stop the entire process

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

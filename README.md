# agg

A command-line utility for preparing code snippets for AI tools by aggregating source code files from a project into a single text file.

## Features

- Recursively traverses directories to collect source files
- Supports filtering by file extensions
- Handles binary files (optional base64 encoding)
- Respects `.gitignore` and `.aggignore` patterns
- Configurable through command line arguments, `.aggconfig`, or global config
- Excludes specified directories
- Outputs to file or stdout
- **Goal-driven context selection**: Use an LLM to intelligently select relevant files and code sections
- **Token budgeting**: Control output size with soft token limits
- **Global configuration**: Set default LLM and budget settings once, use everywhere

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

# Goal-driven context selection with soft budget
agg --goal "Summarize auth flow" --budget 6000 --llm "claude"
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
Usage: agg [OPTIONS] [-- <EXTENSIONS>...] [COMMAND]

Commands:
  config  Manage global configuration

Options:
  -b, --include-binary             Include binary files as base64 encoded strings
  -p, --path <PATH>                Initial path to start searching [default: .]
  -o, --output <OUTPUT>            Output file [default: stdout]
  -e, --exclude-dirs <DIRS>        Directories to exclude
  -d, --debug                      Debug mode, prints arguments and exits
      --goal <GOAL>                Goal describing the context to extract
      --budget <BUDGET>            Soft token budget for goal-driven context
      --llm <LLM>                  LLM provider or command name to invoke
      --llm-cmd <LLM_CMD>          Custom command template to invoke the LLM
      --llm-model <LLM_MODEL>      Optional model name passed to the LLM command
      --llm-debug                  Print debug output for goal-driven LLM selection
      --llm-debug-log <PATH>       Write LLM debug output to a log file
  [EXTENSIONS]                     File extensions to include (space-separated)

Config Subcommands:
  agg config set <KEY> <VALUE>     Set a configuration value
  agg config get <KEY>             Get a configuration value
  agg config list                  List all configuration values
  agg config unset <KEY>           Remove a configuration value
  agg config path                  Show the config file path

Config Keys: llm, llm_cmd, llm_model, budget
```

## Goal-Driven Context Selection

Instead of dumping all files, you can use an LLM to intelligently select only the files and code sections relevant to a specific goal. This is useful when you need focused context for a particular task.

### Quick Start

```bash
# Set your default LLM provider
agg config set llm claude

# Now use goal-driven selection
agg --goal "understand the authentication flow"
```

### How It Works

1. `agg` scans your project and builds a list of available files
2. It sends this list along with your goal to the configured LLM
3. The LLM returns a JSON response selecting relevant files and optionally specific line ranges
4. `agg` outputs only the selected content, respecting the token budget

### Global Configuration

Set default values for LLM settings so you don't need to specify them every time:

```bash
# Set default LLM provider
agg config set llm claude

# Set default token budget
agg config set budget 8000

# Set a custom command template
agg config set llm_cmd "my-llm-wrapper --model gpt-4"

# View current settings
agg config list

# Show config file location
agg config path

# Remove a setting
agg config unset budget
```

Config file locations:
- **Linux**: `~/.config/agg.toml`
- **macOS**: `~/Library/Application Support/agg.toml`
- **Windows**: `%APPDATA%\agg.toml`

### Configuration Precedence

Settings are applied in this order (highest priority first):
1. Command line arguments (`--llm`, `--budget`, etc.)
2. Project-local `.aggconfig` file
3. Global config file
4. Built-in defaults

### LLM Command Options

The LLM can be invoked in several ways:

```bash
# Use a named provider/command directly
agg --goal "fix the bug" --llm claude

# Use a custom command template
agg --goal "fix the bug" --llm-cmd "openai-cli chat"

# Pass a model name (available as $AGG_LLM_MODEL env var)
agg --goal "fix the bug" --llm claude --llm-model claude-3-opus
```

**How prompts are passed to the LLM:**
- The prompt is always available via the `AGG_PROMPT` environment variable
- The prompt is also written to stdin for commands that read from it
- If your command template contains `{{prompt}}`, it's replaced with `"$AGG_PROMPT"` (safe shell expansion)

### Debugging LLM Selection

```bash
# Print debug output to stderr
agg --goal "understand auth" --llm claude --llm-debug

# Write debug output to a file (keeps stdout clean)
agg --goal "understand auth" --llm claude --llm-debug-log debug.log
```

### Example Output (Goal-Driven)

When using goal-driven selection, output includes metadata about the selection:

```
<<<START_CONTEXT:goal="understand auth flow" budget="8000" llm="claude">>>
<<<START_FILE:./src/auth.rs lines="1-50">>>
// Selected code sections here
<<<END_FILE:./src/auth.rs>>>
<<<START_FILE:./src/middleware.rs>>>
// Entire file when relevant
<<<END_FILE:./src/middleware.rs>>>
<<<END_CONTEXT>>>
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

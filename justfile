set shell := ["bash", "-euo", "pipefail", "-c"]
set export := true

root := justfile_directory()

# Format all Rust source files in place
fmt:
    cd "{{root}}" && cargo fmt --all

# Run Clippy lints across the workspace treating warnings as errors
lint:
    cd "{{root}}" && cargo clippy --workspace --all-targets -- -D warnings

# Run workspace tests or tests for a single package
test package="__JUST_WORKSPACE_TEST__":
    cd "{{root}}" && case {{ quote(package) }} in '__JUST_WORKSPACE_TEST__') cargo nextest run --workspace ;; '') echo 'usage: just test [non-empty-crate]' >&2; exit 2 ;; *) cargo nextest run -p {{ quote(package) }} ;; esac

# Run code coverage summary across the workspace
coverage:
    cd "{{root}}" && cargo llvm-cov --workspace --summary-only

# Launch the interactive Storybook component gallery
storybook:
    cd "{{root}}" && cargo run -p storybook

# Run shell and configuration static checks
static:
    cd "{{root}}" && bash scripts/static_checks.sh

# Run format, lint, and workspace tests in sequence
check: fmt lint test

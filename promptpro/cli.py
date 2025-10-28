import sys
from typing import List, Any
try:
    from .promptpro import run_cli as rust_run_cli
except ImportError:
    # If the Rust extension is not available, provide a fallback
    def rust_run_cli(args: List[str]) -> None:
        raise ImportError("Rust promptpro extension not available. Please install with proper Python bindings.")


def run_cli(*args: Any) -> None:
    """
    Call the Rust exposed run_cli function, sending the args to it.
    Mimic rust binary behavior without packaging both lib and bin to a single wheel.
    
    Args:
        *args: Command line arguments to pass to the Rust CLI
    """
    # Convert args to a list of strings
    string_args = [str(arg) for arg in args]
    
    # Call the Rust function
    try:
        rust_run_cli(string_args)
    except Exception as e:
        print(f"Error executing CLI command: {e}", file=sys.stderr)
        sys.exit(1)


def main() -> None:
    """
    Main entry point that mimics the Rust CLI behavior.
    Takes command line arguments and passes them to the Rust implementation.
    """
    # Get command line arguments, excluding the script name
    args = sys.argv[1:] if len(sys.argv) > 1 else []
    
    # Call the Rust CLI function
    run_cli(*args)


if __name__ == "__main__":
    main()

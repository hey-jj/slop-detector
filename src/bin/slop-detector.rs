//! Thin CLI over the library. Reads text from a path argument or stdin and
//! streams the evidence report as JSON to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: slop-detector [FILE]

Reads FILE, or stdin when FILE is absent, and prints the evidence report as JSON.
Inputs over 4 MiB are rejected (exit 40).

Options:
  -h, --help     print this help
  -V, --version  print the version";

/// Fail-closed input cap. Match-dense inputs produce reports proportional
/// to their size; the cap bounds worst-case memory for the CLI. The
/// library itself takes any `&str`.
const MAX_INPUT_BYTES: u64 = 4 * 1024 * 1024;

/// Failure exits: 1 for a read or encoding error, 40 for unsupported input
/// (over the size cap).
enum Failure {
    Read(String),
    Unsupported(String),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Read(msg)) => {
            let _ = writeln!(io::stderr(), "slop-detector: {msg}");
            ExitCode::from(1)
        }
        Err(Failure::Unsupported(msg)) => {
            let _ = writeln!(io::stderr(), "slop-detector: {msg}");
            ExitCode::from(40)
        }
    }
}

fn run() -> Result<(), Failure> {
    let mut path: Option<std::path::PathBuf> = None;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next().map_err(|e| Failure::Read(e.to_string()))? {
        match arg {
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                return write_line(USAGE);
            }
            lexopt::Arg::Short('V') | lexopt::Arg::Long("version") => {
                return write_line(concat!("slop-detector ", env!("CARGO_PKG_VERSION")));
            }
            lexopt::Arg::Value(v) if path.is_none() => {
                path = Some(std::path::PathBuf::from(v));
            }
            arg => return Err(Failure::Read(arg.unexpected().to_string())),
        }
    }

    let text = read_input(path.as_deref())?;
    let report = slop_detector::analyze(&text);
    emit(&report)
}

/// Read the input with the size cap enforced before the bytes are held.
fn read_input(path: Option<&std::path::Path>) -> Result<String, Failure> {
    let over = |n: u64| {
        Failure::Unsupported(format!(
            "input is {n} bytes; the limit is {MAX_INPUT_BYTES} bytes (4 MiB)"
        ))
    };
    let bytes = match path {
        Some(p) => {
            let meta = std::fs::metadata(p)
                .map_err(|e| Failure::Read(format!("failed to read {}: {e}", p.display())))?;
            if meta.len() > MAX_INPUT_BYTES {
                return Err(over(meta.len()));
            }
            std::fs::read(p)
                .map_err(|e| Failure::Read(format!("failed to read {}: {e}", p.display())))?
        }
        None => {
            let mut buf = Vec::new();
            io::stdin()
                .lock()
                .take(MAX_INPUT_BYTES + 1)
                .read_to_end(&mut buf)
                .map_err(|e| Failure::Read(format!("failed to read stdin: {e}")))?;
            buf
        }
    };
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(over(bytes.len() as u64));
    }
    String::from_utf8(bytes).map_err(|_| Failure::Read("input is not valid UTF-8".to_string()))
}

/// Stream the report to stdout without materializing the JSON as a string.
/// A closed pipe (`slop-detector big.txt | head`) is a quiet successful
/// exit, the standard CLI convention.
fn emit(report: &slop_detector::EvidenceReport) -> Result<(), Failure> {
    let stdout = io::stdout().lock();
    let mut w = io::BufWriter::new(stdout);
    match serde_json::to_writer_pretty(&mut w, report) {
        Ok(()) => {}
        Err(e) if e.io_error_kind() == Some(io::ErrorKind::BrokenPipe) => return Ok(()),
        Err(e) => return Err(Failure::Read(format!("failed to write report: {e}"))),
    }
    finish(w.write_all(b"\n").and_then(|()| w.flush()))
}

fn write_line(s: &str) -> Result<(), Failure> {
    let mut stdout = io::stdout().lock();
    finish(writeln!(stdout, "{s}").and_then(|()| stdout.flush()))
}

fn finish(r: io::Result<()>) -> Result<(), Failure> {
    match r {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Failure::Read(format!("failed to write output: {e}"))),
    }
}

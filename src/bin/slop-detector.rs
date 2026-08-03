//! Thin CLI over the library. Reads text from path arguments or stdin and
//! streams the evidence report as JSON to stdout. One path (or stdin)
//! produces the single-document report; two or more paths produce the
//! bundle report, with per-file reports plus cross-file duplication.

use std::io::{self, Read, Write};
use std::process::ExitCode;

const USAGE: &str = "usage: slop-detector [--allow-term TERM]... [FILE]...

Reads each FILE, or stdin when no FILE is given, and prints the evidence
report as JSON. One input produces the single-document report. Two or more
FILEs produce the bundle report: per-file reports plus cross-file
verbatim-duplication evidence.
Each input over 4 MiB is rejected (exit 40).

Options:
  --allow-term TERM  label findings matching this topic term (repeatable);
                     labeled hits stay in the report and leave the residual
                     densities
  -h, --help         print this help
  -V, --version      print the version";

/// Fail-closed input cap, applied per input. Match-dense inputs produce
/// reports proportional to their size; the cap bounds worst-case memory
/// for the CLI. The library itself takes any `&str`.
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
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    let mut opts = slop_detector::AnalyzeOptions::default();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next().map_err(|e| Failure::Read(e.to_string()))? {
        match arg {
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                return write_line(USAGE);
            }
            lexopt::Arg::Short('V') | lexopt::Arg::Long("version") => {
                return write_line(concat!("slop-detector ", env!("CARGO_PKG_VERSION")));
            }
            lexopt::Arg::Long("allow-term") => {
                let term = parser
                    .value()
                    .map_err(|e| Failure::Read(e.to_string()))?
                    .into_string()
                    .map_err(|_| Failure::Read("--allow-term needs UTF-8".to_string()))?;
                opts.allow_terms.push(term);
            }
            lexopt::Arg::Value(v) => {
                paths.push(std::path::PathBuf::from(v));
            }
            arg => return Err(Failure::Read(arg.unexpected().to_string())),
        }
    }

    match paths.len() {
        0 => {
            let text = read_input(None)?;
            emit(&slop_detector::analyze_with(&text, &opts))
        }
        1 => {
            let text = read_input(Some(&paths[0]))?;
            emit(&slop_detector::analyze_with(&text, &opts))
        }
        _ => {
            let docs: Vec<(String, String)> = paths
                .iter()
                .map(|p| Ok((p.display().to_string(), read_input(Some(p))?)))
                .collect::<Result<_, Failure>>()?;
            emit(&slop_detector::analyze_bundle_with(&docs, &opts))
        }
    }
}

/// Read one input with the size cap enforced before the bytes are held.
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
fn emit<T: serde::Serialize>(report: &T) -> Result<(), Failure> {
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

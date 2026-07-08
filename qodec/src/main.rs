//! qodec CLI — encode / decode / bench / aliases / probe.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use qodec::alias::{probe_table, Alphabet};
use qodec::meter::by_name;
use qodec::{bench, container, decode, encode, CodecKind};

#[derive(Parser)]
#[command(
    name = "qodec",
    about = "Q's codec lab: token-aware lossless encode/decode for agent context"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Encode text into a %q1 container (falls back to raw when it doesn't pay).
    Encode(EncodeArgs),
    /// Decode a %q1 container back (unwraps pipelines).
    Decode(IoArgs),
    /// Run every codec over a corpus directory and print a measured table.
    Bench(BenchArgs),
    /// Probe alias candidates against a tokenizer — see what your aliases cost.
    Aliases(AliasArgs),
    /// Emit a paste-ready comprehension probe: legend brief + encoded payload.
    Probe(EncodeArgs),
}

#[derive(Args)]
struct IoArgs {
    /// Input file (stdin when omitted).
    #[arg(short, long)]
    input: Option<PathBuf>,
    /// Output file (stdout when omitted).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct EncodeArgs {
    #[command(flatten)]
    io: IoArgs,
    /// mine | fold | toon | squeeze
    #[arg(long, default_value = "squeeze")]
    codec: String,
    /// auto | glyph | sigil
    #[arg(long, default_value = "auto")]
    alphabet: String,
    /// o200k | cl100k | approx
    #[arg(long, default_value = "o200k")]
    meter: String,
    /// Print a token report to stderr.
    #[arg(long)]
    report: bool,
}

#[derive(Args)]
struct BenchArgs {
    /// Corpus directory of sample files.
    #[arg(long, default_value = "corpus")]
    corpus: PathBuf,
    #[arg(long, default_value = "auto")]
    alphabet: String,
    #[arg(long, default_value = "o200k")]
    meter: String,
}

#[derive(Args)]
struct AliasArgs {
    #[arg(long, default_value = "o200k")]
    meter: String,
    /// How many cheapest candidates to print.
    #[arg(long, default_value_t = 40)]
    top: usize,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Encode(a) => cmd_encode(&a, false),
        Cmd::Probe(a) => cmd_encode(&a, true),
        Cmd::Decode(io) => {
            let text = read_input(&io)?;
            write_output(&io, &decode(&text)?)
        }
        Cmd::Bench(a) => cmd_bench(&a),
        Cmd::Aliases(a) => cmd_aliases(&a),
    }
}

fn cmd_encode(a: &EncodeArgs, probe: bool) -> Result<()> {
    let text = read_input(&a.io)?;
    let meter = by_name(&a.meter)?;
    let kind =
        CodecKind::parse(&a.codec).with_context(|| format!("unknown codec {:?}", a.codec))?;
    let alphabet = Alphabet::parse(&a.alphabet)
        .with_context(|| format!("unknown alphabet {:?}", a.alphabet))?;

    let encoded = encode(&text, kind, meter.as_ref(), alphabet);

    if a.report {
        let tokens_in = meter.count(&text);
        let tokens_cold = meter.count(&encoded);
        let overhead = container::overhead(&encoded, meter.as_ref());
        let warm = tokens_cold.saturating_sub(overhead);
        eprintln!(
            "qodec: {} -> {} tokens (cold, {:+.1}%), body-only {} (warm, {:+.1}%), \
             key overhead {} [{}]",
            tokens_in,
            tokens_cold,
            pct(tokens_in, tokens_cold),
            warm,
            pct(tokens_in, warm),
            overhead,
            meter.name(),
        );
    }

    let payload = if probe {
        probe_wrapper(&encoded)
    } else {
        encoded
    };
    write_output(&a.io, &payload)
}

fn pct(before: usize, after: usize) -> f64 {
    if before == 0 {
        return 0.0;
    }
    100.0 * (after as f64 - before as f64) / before as f64
}

/// A self-contained prompt to test whether a model can *read* the encoded
/// form given only the in-band key. Paste it, then ask questions about the
/// content — compare answers against the raw original.
fn probe_wrapper(encoded: &str) -> String {
    format!(
        "You will receive a payload encoded as a `%q1` container.\n\
         Format: first line `%q1 <codec> ...` (parameters), then legend lines of the\n\
         form `<alias>=<phrase>` (each alias is a short stand-in for that exact phrase),\n\
         then a `%q1 body` line, then the body. `%q1 xN` after a line means that line\n\
         occurs N times in total. A `toon` body is a table: first line is a JSON array\n\
         of keys, each following line is one object, cells are JSON values joined by\n\
         the separator named in the header.\n\
         Mentally decode the payload, then answer questions about its content.\n\
         Never emit alias characters in answers — always use the expanded phrases.\n\n\
         {encoded}"
    )
}

fn cmd_bench(a: &BenchArgs) -> Result<()> {
    let meter = by_name(&a.meter)?;
    let alphabet = Alphabet::parse(&a.alphabet)
        .with_context(|| format!("unknown alphabet {:?}", a.alphabet))?;
    let rows = bench::run(&a.corpus, meter.as_ref(), alphabet)?;
    if rows.is_empty() {
        bail!("no corpus files found in {}", a.corpus.display());
    }
    print!("{}", bench::markdown(&rows, meter.name(), alphabet.label()));
    if rows.iter().any(|r| r.roundtrip == "FAIL") {
        bail!("roundtrip FAILURE present — that's a bug");
    }
    Ok(())
}

fn cmd_aliases(a: &AliasArgs) -> Result<()> {
    let meter = by_name(&a.meter)?;
    println!(
        "alias candidates under `{}` (cheapest first):",
        meter.name()
    );
    println!("| alias | kind | tokens |");
    println!("|---|---|---:|");
    for row in probe_table(meter.as_ref(), a.top) {
        println!("| {} | {} | {} |", row.alias, row.kind, row.cost);
    }
    Ok(())
}

fn read_input(io: &IoArgs) -> Result<String> {
    match &io.input {
        Some(path) => {
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            Ok(buf)
        }
    }
}

fn write_output(io: &IoArgs, text: &str) -> Result<()> {
    match &io.output {
        Some(path) => fs::write(path, text).with_context(|| format!("writing {}", path.display())),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

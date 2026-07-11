//! qodec CLI — encode / decode / bench / aliases / probe.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};

use qodec::alias::{probe_table, Alphabet};
use qodec::meter::by_name;
use qodec::{bench, container, encode, CodecKind};

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
    Decode(DecodeArgs),
    /// Freeze the heaviest profile phrases into an extern legend file — a
    /// stable dictionary for a cached prompt prefix (CLAUDE.md / system
    /// prompt). Artifacts pin its checksum; decode fails closed on drift.
    Legend(LegendArgs),
    /// Slice a record array out of a JSON document: descend by --key, keep
    /// records matching every --where, emit a compact JSON array (feed it
    /// raw, or pipe into `encode --codec toon`).
    Slice(SliceArgs),
    /// Learn a per-repo redundancy profile from files: repeated phrases and
    /// templates accumulate across runs; `encode --profile` probes them
    /// first. Acceptance stays measured — a stale profile costs probes,
    /// never bytes.
    Learn(LearnArgs),
    /// Run every codec over a corpus directory and print a measured table.
    Bench(BenchArgs),
    /// Probe alias candidates against a tokenizer — see what your aliases cost.
    Aliases(AliasArgs),
    /// Emit a paste-ready comprehension probe: legend brief + encoded payload.
    Probe(EncodeArgs),
    /// Perplexity gate: score raw vs encoded under a local LM endpoint —
    /// cheap comprehension proxy before judge runs. See docs/token-codec.md.
    Ppl(PplArgs),
    /// Comprehension A/B: emit paired raw/encoded prompts, grade answers.
    #[command(subcommand)]
    Ab(AbCmd),
}

#[derive(Subcommand)]
enum AbCmd {
    /// Write `raw.prompt.txt` and `encoded.prompt.txt` into --out-dir.
    Emit(AbEmitArgs),
    /// Grade an answers JSON against a questions file.
    Grade(AbGradeArgs),
}

#[derive(Args)]
struct AbEmitArgs {
    /// Payload file.
    #[arg(short, long)]
    input: PathBuf,
    /// Questions JSON (array of {id, question, accept}).
    #[arg(long)]
    questions: PathBuf,
    #[arg(long, default_value = "deep")]
    codec: String,
    #[arg(long, default_value = "auto")]
    alphabet: String,
    #[arg(long, default_value = "o200k")]
    meter: String,
    #[arg(long)]
    out_dir: PathBuf,
}

#[derive(Args)]
struct AbGradeArgs {
    #[arg(long)]
    questions: PathBuf,
    /// Model answers (JSON object id->answer; chatty text around it is ok).
    #[arg(long)]
    answers: PathBuf,
    /// The prompt the model answered from; adds the accuracy-per-1k-tokens
    /// line (the TOON-benchmark normalization) so raw vs encoded runs
    /// compare on cost, not just score.
    #[arg(long)]
    prompt: Option<PathBuf>,
    /// o200k | cl100k | approx
    #[arg(long, default_value = "o200k")]
    meter: String,
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
    /// mine | deep | fold | toon | grep | diag | tmpl | squeeze
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
    /// Redundancy profile from `qodec learn`; its phrases are probed first.
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Extern legend (`qodec legend`): substitute its phrases without
    /// paying for legend lines — the reader holds the key in a cached
    /// prefix. The artifact pins the file's checksum.
    #[arg(long)]
    extern_legend: Option<PathBuf>,
}

#[derive(Args)]
struct DecodeArgs {
    #[command(flatten)]
    io: IoArgs,
    /// The extern legend the artifact was encoded against (`ext` artifacts
    /// refuse to decode without the exact file).
    #[arg(long)]
    extern_legend: Option<PathBuf>,
}

#[derive(Args)]
struct LegendArgs {
    /// Profile from `qodec learn`.
    #[arg(long)]
    profile: PathBuf,
    /// How many phrases to freeze.
    #[arg(long, default_value_t = 48)]
    top: usize,
    /// o200k | cl100k | approx
    #[arg(long, default_value = "o200k")]
    meter: String,
    /// Output file (stdout when omitted).
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct LearnArgs {
    /// Files to learn from (repeatable).
    #[arg(short, long)]
    input: Vec<PathBuf>,
    /// Or every file in this directory (non-recursive, like bench).
    #[arg(long)]
    corpus: Option<PathBuf>,
    /// Profile to create or update.
    #[arg(long)]
    profile: PathBuf,
}

#[derive(Args)]
struct SliceArgs {
    #[command(flatten)]
    io: IoArgs,
    /// Dotted path to the record array (omit when the root is the array).
    #[arg(long, default_value = "")]
    key: String,
    /// key=value | key!=value | key~substring — repeatable, all must match.
    #[arg(long = "where", value_name = "CLAUSE")]
    clauses: Vec<String>,
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

#[derive(Args)]
struct PplArgs {
    /// Input file (stdin when omitted).
    #[arg(short, long)]
    input: Option<PathBuf>,
    /// mine | deep | fold | toon | grep | diag | tmpl | squeeze
    #[arg(long, default_value = "squeeze")]
    codec: String,
    #[arg(long, default_value = "auto")]
    alphabet: String,
    #[arg(long, default_value = "o200k")]
    meter: String,
    /// OpenAI-compatible legacy completions endpoint with echo+logprobs
    /// (vLLM; FastContext served locally). Env: QODEC_PPL_URL.
    #[arg(long, env = "QODEC_PPL_URL")]
    url: String,
    /// Served model name. Env: QODEC_PPL_MODEL.
    #[arg(long, env = "QODEC_PPL_MODEL", default_value = "fastcontext")]
    model: String,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Encode(a) => cmd_encode(&a, false),
        Cmd::Probe(a) => cmd_encode(&a, true),
        Cmd::Decode(a) => {
            let text = read_input(&a.io)?;
            let extern_legend = a
                .extern_legend
                .as_deref()
                .map(qodec::legend::ExternLegend::load)
                .transpose()?;
            write_output(
                &a.io,
                &qodec::decode_with_extern(&text, extern_legend.as_ref())?,
            )
        }
        Cmd::Legend(a) => cmd_legend(&a),
        Cmd::Slice(a) => cmd_slice(&a),
        Cmd::Learn(a) => cmd_learn(&a),
        Cmd::Bench(a) => cmd_bench(&a),
        Cmd::Aliases(a) => cmd_aliases(&a),
        Cmd::Ppl(a) => cmd_ppl(&a),
        Cmd::Ab(AbCmd::Emit(a)) => cmd_ab_emit(&a),
        Cmd::Ab(AbCmd::Grade(a)) => cmd_ab_grade(&a),
    }
}

fn cmd_ab_emit(a: &AbEmitArgs) -> Result<()> {
    let payload =
        fs::read_to_string(&a.input).with_context(|| format!("reading {}", a.input.display()))?;
    let questions_text = fs::read_to_string(&a.questions)
        .with_context(|| format!("reading {}", a.questions.display()))?;
    let questions = qodec::ab::parse_questions(&questions_text)?;

    let meter = by_name(&a.meter)?;
    let kind =
        CodecKind::parse(&a.codec).with_context(|| format!("unknown codec {:?}", a.codec))?;
    let alphabet = Alphabet::parse(&a.alphabet)
        .with_context(|| format!("unknown alphabet {:?}", a.alphabet))?;
    let encoded = encode(&payload, kind, meter.as_ref(), alphabet);
    let outcome = container::parse(&encoded)
        .map(|c| c.codec)
        .unwrap_or_else(|_| "?".to_string());
    // Fail closed: a raw fallback would compare raw text against raw-in-a-
    // container — an "A/B" that never exercises the notation and can only
    // report fake encoded-side success (Codex review on PR #28).
    if outcome == "raw" {
        bail!(
            "codec {:?} fell back to raw on this payload — the A/B would not \
             exercise the notation; pick a more repetitive payload or another codec",
            a.codec
        );
    }

    fs::create_dir_all(&a.out_dir).with_context(|| format!("creating {}", a.out_dir.display()))?;
    let raw_prompt = qodec::ab::prompt("", &payload, &questions);
    let enc_prompt = qodec::ab::prompt(qodec::ab::notation_brief(), &encoded, &questions);
    fs::write(a.out_dir.join("raw.prompt.txt"), &raw_prompt)?;
    fs::write(a.out_dir.join("encoded.prompt.txt"), &enc_prompt)?;
    eprintln!(
        "qodec ab: emitted prompts to {} (payload {} -> {} tokens encoded as {} [{}])",
        a.out_dir.display(),
        meter.count(&payload),
        meter.count(&encoded),
        outcome,
        meter.name(),
    );
    Ok(())
}

fn cmd_ab_grade(a: &AbGradeArgs) -> Result<()> {
    let questions_text = fs::read_to_string(&a.questions)
        .with_context(|| format!("reading {}", a.questions.display()))?;
    let questions = qodec::ab::parse_questions(&questions_text)?;
    let answers = fs::read_to_string(&a.answers)
        .with_context(|| format!("reading {}", a.answers.display()))?;
    let rows = qodec::ab::grade(&questions, &answers)?;
    let correct = rows.iter().filter(|r| r.correct).count();
    for r in &rows {
        println!(
            "{} {}: {}",
            if r.correct { "PASS" } else { "FAIL" },
            r.id,
            r.answer
        );
    }
    println!("score: {correct}/{}", rows.len());
    if let Some(prompt_path) = &a.prompt {
        let prompt = fs::read_to_string(prompt_path)
            .with_context(|| format!("reading {}", prompt_path.display()))?;
        let meter = by_name(&a.meter)?;
        let tokens = meter.count(&prompt);
        println!(
            "prompt: {tokens} tokens [{}]   accuracy/1k tokens: {:.1}",
            meter.name(),
            qodec::ab::accuracy_per_1k(correct, rows.len(), tokens),
        );
    }
    Ok(())
}

fn cmd_ppl(a: &PplArgs) -> Result<()> {
    let text = read_input(&IoArgs {
        input: a.input.clone(),
        output: None,
    })?;
    let meter = by_name(&a.meter)?;
    let kind =
        CodecKind::parse(&a.codec).with_context(|| format!("unknown codec {:?}", a.codec))?;
    let alphabet = Alphabet::parse(&a.alphabet)
        .with_context(|| format!("unknown alphabet {:?}", a.alphabet))?;

    let encoded = encode(&text, kind, meter.as_ref(), alphabet);
    let cfg = qodec::ppl::PplConfig {
        url: a.url.clone(),
        model: a.model.clone(),
    };
    let report = qodec::ppl::compare(&cfg, &text, &encoded)?;
    println!(
        "raw:     ppl {:8.2} over {} tokens\n\
         encoded: ppl {:8.2} over {} tokens ({} tokens by {})\n\
         ratio:   {:.3} -> {}",
        report.raw.perplexity,
        report.raw.tokens,
        report.encoded.perplexity,
        report.encoded.tokens,
        meter.count(&encoded),
        meter.name(),
        report.ratio(),
        report.verdict(),
    );
    Ok(())
}

fn cmd_encode(a: &EncodeArgs, probe: bool) -> Result<()> {
    let text = read_input(&a.io)?;
    let meter = by_name(&a.meter)?;
    let kind =
        CodecKind::parse(&a.codec).with_context(|| format!("unknown codec {:?}", a.codec))?;
    let alphabet = Alphabet::parse(&a.alphabet)
        .with_context(|| format!("unknown alphabet {:?}", a.alphabet))?;
    let seeds = match &a.profile {
        Some(path) => {
            let profile = qodec::profile::Profile::load(path)?;
            qodec::Seeds {
                phrases: profile.seed_phrases(64),
                templates: profile.seed_templates(64),
            }
        }
        None => qodec::Seeds::default(),
    };

    let extern_legend = a
        .extern_legend
        .as_deref()
        .map(qodec::legend::ExternLegend::load)
        .transpose()?;
    if probe && extern_legend.is_some() {
        bail!(
            "probe teaches the in-band key; an extern legend is out-of-band \
             by design — probe without --extern-legend"
        );
    }

    let (to_encode, substitution) = match &extern_legend {
        Some(legend) => {
            let sub = qodec::legend::substitute(&text, legend, meter.as_ref());
            (sub.text.clone(), Some(sub))
        }
        None => (text.clone(), None),
    };
    let inner = qodec::encode_seeded(&to_encode, kind, meter.as_ref(), alphabet, &seeds);
    let encoded = match (&extern_legend, &substitution) {
        (Some(legend), Some(sub)) => {
            qodec::legend::wrap_if_used(inner, legend, &sub.used, meter.as_ref(), &text)
        }
        _ => inner,
    };

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

fn cmd_legend(a: &LegendArgs) -> Result<()> {
    let profile = qodec::profile::Profile::load(&a.profile)?;
    let meter = by_name(&a.meter)?;
    let text = qodec::legend::generate(&profile, meter.as_ref(), a.top)?;
    let entries = text.lines().filter(|l| l.contains('=')).count();
    match &a.output {
        Some(path) => {
            fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            eprintln!(
                "qodec legend: {entries} entries -> {} ({} tokens as cached-prefix key)",
                path.display(),
                meter.count(&text),
            );
        }
        None => print!("{text}"),
    }
    Ok(())
}

fn cmd_learn(a: &LearnArgs) -> Result<()> {
    // Harvest memory is proportional to input size; big blobs (SARIF and
    // friends) get a capped, logged prefix instead of an OOM.
    const LEARN_CAP_BYTES: usize = 4 * 1024 * 1024;

    let mut files: Vec<PathBuf> = a.input.clone();
    if let Some(dir) = &a.corpus {
        let mut listed: Vec<PathBuf> = fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        listed.sort();
        files.extend(listed);
    }
    if files.is_empty() {
        bail!("nothing to learn from — pass -i <file> and/or --corpus <dir>");
    }

    let mut profile = qodec::profile::Profile::load(&a.profile)?;
    let mut learned = 0usize;
    for path in &files {
        // The cap is applied at the read itself — a multi-GB blob never
        // reaches memory (Codex review on PR #34).
        let (text, capped) = match qodec::profile::read_capped(path, LEARN_CAP_BYTES) {
            Ok(read) => read,
            Err(err) => {
                eprintln!("qodec learn: skipping {} ({err})", path.display());
                continue;
            }
        };
        if capped {
            eprintln!(
                "qodec learn: {} capped to its first {} bytes",
                path.display(),
                text.len(),
            );
        }
        profile.learn_from(&text);
        learned += 1;
    }
    profile.save(&a.profile)?;
    eprintln!(
        "qodec learn: {learned} file(s) -> {} ({} phrases, {} templates, {} runs total)",
        a.profile.display(),
        profile.phrase_count(),
        profile.template_count(),
        profile.runs,
    );
    Ok(())
}

fn cmd_slice(a: &SliceArgs) -> Result<()> {
    let text = read_input(&a.io)?;
    let clauses = a
        .clauses
        .iter()
        .map(|s| qodec::slice::Clause::parse(s))
        .collect::<Result<Vec<_>>>()?;
    let s = qodec::slice::slice(&text, &a.key, &clauses)?;
    eprintln!("qodec slice: kept {} / {} records", s.kept, s.total);
    write_output(&a.io, &s.body)
}

fn pct(before: usize, after: usize) -> f64 {
    if before == 0 {
        return 0.0;
    }
    100.0 * (after as f64 - before as f64) / before as f64
}

/// A self-contained prompt to test whether a model can *read* the encoded
/// form given only the in-band key. Paste it, then ask questions about the
/// content — compare answers against the raw original. The notation text is
/// `ab::notation_brief()` verbatim, so probe and A/B can never drift apart
/// (Codex review on PR #33).
fn probe_wrapper(encoded: &str) -> String {
    format!("{}\n\n{encoded}", qodec::ab::notation_brief())
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

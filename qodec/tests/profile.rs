//! Profile semantics — the memory must merge deterministically across
//! runs, survive disk, and provably transfer knowledge the fast miner
//! cannot rediscover on its own.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::profile::Profile;
use qodec::{encode, encode_seeded, CodecKind, Seeds};

/// Log-ish lines sharing one 14-word stem — far beyond the word-miner's
/// MAX_WORDS window, so plain mine can only tile it from sub-spans while a
/// profile seed replaces it whole.
fn stem_lines(tag: &str) -> String {
    let mut text = String::new();
    for i in 0..24 {
        text.push_str(&format!(
            "  the quick brown fox jumps over the lazy dog while the sleepy cat watches {tag}{i}\n"
        ));
    }
    text
}

#[test]
fn profile_accumulates_and_roundtrips_disk() -> Result<()> {
    let mut profile = Profile::default();
    profile.learn_from(&stem_lines("alpha"));
    profile.learn_from(&stem_lines("beta"));
    anyhow::ensure!(profile.runs == 2, "two texts learned");
    anyhow::ensure!(
        profile.phrase_count() > 0 && profile.template_count() > 0,
        "both families harvested"
    );

    let path = std::env::temp_dir().join("qodec-profile-roundtrip-test.json");
    profile.save(&path)?;
    anyhow::ensure!(
        !path.with_extension("json.tmp").exists(),
        "atomic save must leave no temp file behind"
    );
    let back = Profile::load(&path)?;
    std::fs::remove_file(&path).ok();
    anyhow::ensure!(
        back.runs == profile.runs
            && back.phrase_count() == profile.phrase_count()
            && back.template_count() == profile.template_count(),
        "disk roundtrip must preserve the profile"
    );
    anyhow::ensure!(
        back.seed_phrases(64) == profile.seed_phrases(64),
        "seed order must be deterministic across save/load"
    );
    Ok(())
}

#[test]
fn missing_profile_is_empty_and_bad_version_errors() -> Result<()> {
    let missing = Profile::load(std::path::Path::new(
        "/nonexistent/qodec-no-such-profile.json",
    ))?;
    anyhow::ensure!(missing.runs == 0, "missing file starts a fresh profile");

    let path = std::env::temp_dir().join("qodec-profile-badversion-test.json");
    std::fs::write(&path, r#"{"v": 99, "runs": 1}"#)?;
    let result = Profile::load(&path);
    std::fs::remove_file(&path).ok();
    anyhow::ensure!(
        result.is_err(),
        "an unknown profile version must refuse, not misread"
    );
    Ok(())
}

#[test]
fn seeds_transfer_what_the_word_miner_cannot_see() -> Result<()> {
    // The 14-word stem exceeds MAX_WORDS, so the fast word miner can only
    // tile it from sub-spans — but tmpl clustering learns it whole as a
    // template part on file A, and the profile seeds it into file B's
    // encode. The guaranteed invariants: the stem transfers, gets probed,
    // commits as ONE legend entry, and the artifact stays byte-lossless
    // and smaller than the input. NOT guaranteed: beating plain mine —
    // greedy commit order and context-dependent alias glyph costs can go
    // either way (measured: plain's nested two-entry tiling won by a
    // 1-token-per-row glyph difference on this very sample).
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&stem_lines("alpha"));

    let phrases = profile.seed_phrases(64);
    anyhow::ensure!(
        phrases.iter().any(|s| s.split_whitespace().count() >= 14),
        "profile must carry the long template stem, got {phrases:?}"
    );
    let seeds = Seeds {
        phrases,
        ..Seeds::default()
    };

    let other = stem_lines("omega");
    let seeded = encode_seeded(&other, CodecKind::Mine, &meter, Alphabet::Auto, &seeds);
    anyhow::ensure!(seeded.starts_with("%q1 mine"), "seeded encode must commit");
    let stem = "the quick brown fox jumps over the lazy dog while the sleepy cat watches";
    anyhow::ensure!(
        seeded.lines().take(30).any(|l| l.ends_with(&format!("={stem}"))),
        "the transferred stem must land in the legend as one entry"
    );
    anyhow::ensure!(
        meter.count(&seeded) < meter.count(&other),
        "seeded artifact must beat the raw input"
    );
    let back = qodec::decode(&seeded)?;
    anyhow::ensure!(back == other, "seeded encode stays byte-lossless");
    // Plain mine still works alongside for comparison runs.
    let plain = encode(&other, CodecKind::Mine, &meter, Alphabet::Auto);
    anyhow::ensure!(plain.starts_with("%q1 mine"), "plain baseline still commits");
    Ok(())
}

/// Two line families with the same seg shape sharing exactly 4 of 6 words
/// (0.667 ≥ SIMILARITY) — a greedy single-pass merges them into one
/// two-slot mongrel template, which is Drain's known first-fit weakness.
fn family(kind: &str, noun: &str, n: usize) -> String {
    let mut text = String::new();
    for i in 0..n {
        text.push_str(&format!("worker thread pool {kind} {noun}{i} spawned\n"));
    }
    text
}

#[test]
fn tmpl_seeds_rescue_misrouted_templates_and_pin_legend_bytes() -> Result<()> {
    // Learned from separate files, the profile holds two clean one-slot
    // templates. On a mixed file the plain pass cannot: the second
    // family's first line clears the similarity bar against the first
    // family's cluster and erodes it into `worker thread pool ¿ ¿
    // spawned`, so every row pays an extra slot value. Seeded clustering
    // routes each line to its sealed template first.
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&family("delta", "task", 16));
    profile.learn_from(&family("epsilon", "job", 16));
    let templates = profile.seed_templates(64);
    anyhow::ensure!(
        templates
            .iter()
            .filter(|parts| {
                parts.first().is_some_and(|p| {
                    p == "worker thread pool delta task" || p == "worker thread pool epsilon job"
                })
            })
            .count()
            == 2,
        "profile must hold both clean sub-word-refined templates, got {templates:?}"
    );
    let seeds = Seeds {
        templates,
        ..Seeds::default()
    };

    // Interleave the families the way a real mixed log would.
    let mixed: String = family("delta", "task", 16)
        .lines()
        .zip(family("epsilon", "job", 16).lines())
        .flat_map(|(a, b)| [a, "\n", b, "\n"])
        .collect();

    let plain = encode(&mixed, CodecKind::Tmpl, &meter, Alphabet::Auto);
    anyhow::ensure!(plain.starts_with("%q1 tmpl"), "plain must commit: {plain:?}");
    anyhow::ensure!(
        plain.contains("=worker thread pool ¿ ¿ spawned"),
        "plain first-fit must produce the merged two-slot template"
    );

    let seeded = encode_seeded(&mixed, CodecKind::Tmpl, &meter, Alphabet::Auto, &seeds);
    anyhow::ensure!(seeded.starts_with("%q1 tmpl"), "seeded must commit");
    anyhow::ensure!(
        seeded.contains("=worker thread pool delta task¿ spawned")
            && seeded.contains("=worker thread pool epsilon job¿ spawned"),
        "seeded legend must pin both sub-word profile templates byte-exactly: {seeded:?}"
    );
    anyhow::ensure!(
        meter.count(&seeded) < meter.count(&plain),
        "one slot value per row instead of two must measure smaller: seeded {} vs plain {}",
        meter.count(&seeded),
        meter.count(&plain),
    );
    let back = qodec::decode(&seeded)?;
    anyhow::ensure!(back == mixed, "seeded tmpl stays byte-lossless");
    Ok(())
}

#[test]
fn unrelated_template_seeds_change_nothing() -> Result<()> {
    // Seeds that match no input line must leave the artifact byte-identical
    // — the min(seeded, plain) gate plus empty sealed clusters guarantee a
    // stale profile costs a pass, never bytes.
    let meter = Bpe::o200k()?;
    let mut profile = Profile::default();
    profile.learn_from(&family("delta", "task", 16));
    let seeds = Seeds {
        templates: profile.seed_templates(64),
        ..Seeds::default()
    };
    let prose = "the meeting moved to thursday because the room was double booked\n\
                 the meeting moved to friday because the projector was broken\n"
        .repeat(4);
    let plain = encode(&prose, CodecKind::Tmpl, &meter, Alphabet::Auto);
    let seeded = encode_seeded(&prose, CodecKind::Tmpl, &meter, Alphabet::Auto, &seeds);
    anyhow::ensure!(seeded == plain, "unmatched seeds must not change the artifact");
    Ok(())
}

#[test]
fn read_capped_bounds_the_read_and_respects_char_boundaries() -> Result<()> {
    use qodec::profile::read_capped;
    let dir = std::env::temp_dir();

    // A small file arrives whole, uncapped.
    let small = dir.join("qodec-readcap-small-test.txt");
    std::fs::write(&small, "hello")?;
    let (text, capped) = read_capped(&small, 16)?;
    std::fs::remove_file(&small).ok();
    anyhow::ensure!(text == "hello" && !capped, "small file must pass through");

    // The cap slicing a multibyte char drops that char, never mangles it.
    let multi = dir.join("qodec-readcap-multibyte-test.txt");
    std::fs::write(&multi, "abcd码码码")?;
    let (text, capped) = read_capped(&multi, 6)?;
    std::fs::remove_file(&multi).ok();
    anyhow::ensure!(capped && text == "abcd", "sliced char must be dropped, got {text:?}");

    // Binary content is refused (same semantics as the uncapped read had).
    let bin = dir.join("qodec-readcap-binary-test.bin");
    std::fs::write(&bin, [0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05])?;
    let refused = read_capped(&bin, 4);
    std::fs::remove_file(&bin).ok();
    anyhow::ensure!(refused.is_err(), "binary must be refused, not mangled");
    Ok(())
}

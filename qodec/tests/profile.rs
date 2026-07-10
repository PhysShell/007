//! Profile semantics — the memory must merge deterministically across
//! runs, survive disk, and provably transfer knowledge the fast miner
//! cannot rediscover on its own.

use anyhow::Result;

use qodec::alias::Alphabet;
use qodec::meter::{Bpe, TokenMeter};
use qodec::profile::Profile;
use qodec::{encode, encode_seeded, CodecKind};

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

    let seeds = profile.seed_phrases(64);
    anyhow::ensure!(
        seeds.iter().any(|s| s.split_whitespace().count() >= 14),
        "profile must carry the long template stem, got {seeds:?}"
    );

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

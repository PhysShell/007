//! Perplexity-gate wire test against a canned OpenAI-compatible endpoint.

use std::io::{Read, Write};
use std::net::TcpListener;

use anyhow::{Context, Result};

use qodec::ppl::{compare, PplConfig};

/// One-shot HTTP server: answers `hits` requests with the given JSON bodies.
fn mock_server(bodies: Vec<String>) -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").context("binding mock server")?;
    let addr = listener.local_addr().context("mock server addr")?;
    std::thread::spawn(move || {
        for body in bodies {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            // Read headers, then the announced body length.
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            let header_end = loop {
                let Ok(n) = stream.read(&mut chunk) else {
                    return;
                };
                buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break pos + 4;
                }
                if n == 0 {
                    return;
                }
            };
            let headers = String::from_utf8_lossy(buf.get(..header_end).unwrap_or_default())
                .to_ascii_lowercase();
            let content_length: usize = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(0);
            while buf.len() < header_end + content_length {
                let Ok(n) = stream.read(&mut chunk) else {
                    return;
                };
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Ok(format!("http://{addr}/v1/completions"))
}

#[test]
fn compare_computes_perplexity_ratio_from_echo_logprobs() -> Result<()> {
    // raw: mean logprob -1.0 -> ppl e^1; encoded: mean -2.0 -> ppl e^2.
    let raw_body =
        r#"{"choices":[{"text":"x","logprobs":{"token_logprobs":[null,-1.0,-1.0,-1.0]}}]}"#;
    let enc_body = r#"{"choices":[{"text":"x","logprobs":{"token_logprobs":[null,-2.0,-2.0]}}]}"#;
    let url = mock_server(vec![raw_body.to_string(), enc_body.to_string()])?;

    let cfg = PplConfig {
        url,
        model: "fastcontext".to_string(),
    };
    let report = compare(&cfg, "raw text", "encoded text")?;

    anyhow::ensure!(report.raw.tokens == 3, "raw scored tokens");
    anyhow::ensure!(report.encoded.tokens == 2, "encoded scored tokens");
    let expected_ratio = (2.0f64).exp() / (1.0f64).exp();
    anyhow::ensure!(
        (report.ratio() - expected_ratio).abs() < 1e-9,
        "ratio {} != {expected_ratio}",
        report.ratio()
    );
    anyhow::ensure!(
        report.verdict() == "borderline — run the judge A/B",
        "verdict for ratio e: {}",
        report.verdict()
    );
    Ok(())
}

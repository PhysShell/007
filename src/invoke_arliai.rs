//! Arli AI backend for `o7 invoke` — the HTTP layer and response
//! extraction, nothing else.
//!
//! This module deliberately knows nothing about run dirs, meta.json, or
//! status vocabulary: it performs exactly one HTTPS POST to a
//! compile-time-fixed endpoint and returns a raw, classifiable outcome
//! (`ArliOutcome`); `invoke.rs::classify_arli` owns the mapping to
//! `InvokeStatus`. The normative contract (endpoint binding, redirect ban,
//! request shape, classification matrix, key-handling boundary) is
//! `docs/o7-invoke.md`, "Arli AI backend" — written before this code.
//!
//! Key handling: the API key arrives as a `&str` parameter, is formatted
//! into the `Authorization` header value, and is never stored in any
//! struct, never part of any returned outcome, and never formatted into
//! any error detail (ureq's error types carry no request headers). ureq
//! logs through the `log` facade and its wire-level TRACE is unredacted;
//! the enforced boundary against that is `run_arliai`'s fail-closed
//! preflight (refuse dispatch when the global max level admits TRACE) —
//! "o7 currently installs no logger" is context, not the guarantee.

use serde_json::Value;
use std::time::Duration;

/// The one endpoint this backend will ever talk to. A compile-time
/// constant on purpose: no flag, env var, or config may repoint the
/// `Authorization`-carrying call (docs/o7-invoke.md, "Endpoint binding").
/// Tests exercise the HTTP path through the private `call_url` seam with a
/// local listener; production code has no way to reach that seam with a
/// different URL.
const ENDPOINT: &str = "https://api.arliai.com/v1/chat/completions";

/// Explicit response-body bound (docs/o7-invoke.md, "Response handling").
/// ureq happens to default to a similar cap, but a runtime boundary must
/// not exist only as an inherited SDK default: the limit is declared here,
/// passed explicitly, and exceeding it classifies as its own outcome
/// (`BLOCKED_PROVIDER` / `response_too_large`), never a generic transport
/// error.
pub(crate) const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;

/// One call's raw outcome, prior to any status classification.
pub(crate) enum ArliOutcome {
    /// The server answered. Any status — 2xx and non-2xx alike land here
    /// (`http_status_as_error(false)`), and so does an un-followed 3xx
    /// (`max_redirects(0)` + `max_redirects_will_error(false)`). `body` is
    /// the raw response body, byte-for-byte (written to `stdout.raw`).
    Http { status: u16, body: Vec<u8> },
    /// The configured global timeout elapsed before a complete response.
    TimedOut,
    /// ureq refused a redirect on the error path (`RedirectFailed` /
    /// `TooManyRedirects`) rather than returning the 3xx response. Kept
    /// distinct from `Transport` so the classifier can still say
    /// `error_kind: "redirect"` no matter which path a 3xx takes.
    Redirect { detail: String },
    /// The response body exceeded the explicit `MAX_RESPONSE_BYTES` bound
    /// (`ureq::Error::BodyExceedsLimit` while draining the body).
    TooLarge { limit: u64 },
    /// DNS, TLS, refused/reset connections, protocol violations — no HTTP
    /// response to classify. `detail` is ureq's error rendering (which
    /// carries no request headers, so no key can leak through it).
    Transport { detail: String },
}

/// Perform the one production call. See `call_url` for everything real;
/// this wrapper exists so the fixed endpoint is the ONLY url production
/// code can pass.
pub(crate) fn call(
    prompt: &str,
    schema_for_api: &Value,
    model: &str,
    api_key: &str,
    timeout: Duration,
) -> ArliOutcome {
    call_url(
        ENDPOINT,
        prompt,
        schema_for_api,
        model,
        api_key,
        timeout,
        MAX_RESPONSE_BYTES,
    )
}

/// The documented Arli request shape (docs/o7-invoke.md): `json_schema`
/// response_format with a bare `schema` object and NO OpenAI-flavored
/// `name` wrapper; `tool_choice: "none"`; `stream: false`;
/// `include_reasoning: false` explicitly — Arli documents it as defaulting
/// to true, and reasoning output is not normative evidence for this
/// primitive (it would only inflate latency and `stdout.raw`). Split out
/// so the shape is unit-testable without a socket.
fn request_body(prompt: &str, schema_for_api: &Value, model: &str) -> String {
    serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": prompt }],
        "response_format": { "type": "json_schema", "json_schema": { "schema": schema_for_api } },
        "tool_choice": "none",
        "stream": false,
        "include_reasoning": false,
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)] // the one seam between call() and the tests
fn call_url(
    url: &str,
    prompt: &str,
    schema_for_api: &Value,
    model: &str,
    api_key: &str,
    timeout: Duration,
    max_body_bytes: u64,
) -> ArliOutcome {
    let body = request_body(prompt, schema_for_api, model);

    // Every knob here is load-bearing for the docs contract:
    // - `http_status_as_error(false)`: one classification path — the caller
    //   sees every HTTP status as a status, never as an error variant;
    // - `max_redirects(0)` + `max_redirects_will_error(false)`: a 3xx is
    //   returned as-is, never followed (endpoint binding);
    // - `timeout_global`: the whole transaction is bounded by
    //   `--timeout-secs`, same meaning as the CLI backends' kill timer;
    // - `proxy(None)`: proxy env vars are deliberately ignored — the
    //   connection is direct to the pinned endpoint or it doesn't happen.
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .max_redirects_will_error(false)
        .timeout_global(Some(timeout))
        .proxy(None)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    let result = agent
        .post(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .send(body.as_str());

    match result {
        Ok(mut resp) => {
            let status = resp.status().as_u16();
            match resp
                .body_mut()
                .with_config()
                .limit(max_body_bytes)
                .read_to_vec()
            {
                Ok(bytes) => ArliOutcome::Http {
                    status,
                    body: bytes,
                },
                Err(e) => outcome_from_error(e),
            }
        }
        Err(e) => outcome_from_error(e),
    }
}

fn outcome_from_error(e: ureq::Error) -> ArliOutcome {
    match e {
        ureq::Error::Timeout(_) => ArliOutcome::TimedOut,
        ureq::Error::Io(ref io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            ArliOutcome::TimedOut
        }
        ureq::Error::RedirectFailed | ureq::Error::TooManyRedirects => ArliOutcome::Redirect {
            detail: e.to_string(),
        },
        ureq::Error::BodyExceedsLimit(limit) => ArliOutcome::TooLarge { limit },
        other => ArliOutcome::Transport {
            detail: other.to_string(),
        },
    }
}

/// Why a 2xx body failed to yield a candidate JSON value. Each maps 1:1 to
/// a documented `error_kind` (docs/o7-invoke.md classification matrix);
/// all of them classify as `FAIL_INVALID_OUTPUT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtractError {
    /// Body is not JSON, or the chat-completions envelope shape is wrong
    /// (`choices` missing/not an array, `message` missing, `content`
    /// present but not a string).
    BadEnvelope,
    /// `choices` is an empty array.
    EmptyChoices,
    /// `message.tool_calls` is present and non-empty — the model tried to
    /// call a tool despite `tool_choice: "none"`. Nothing here would ever
    /// execute it; rejecting is belt and braces.
    ToolCallsPresent,
    /// `message.content` is `null` or absent.
    NullContent,
    /// `message.content` is a string but not bare JSON. No fence
    /// tolerance, deliberately: with server-side constrained decoding
    /// requested, a fenced answer is an anomaly worth failing loudly
    /// (`stdout.raw` keeps the evidence).
    BadContentJson,
}

impl ExtractError {
    pub(crate) fn error_kind(self) -> &'static str {
        match self {
            ExtractError::BadEnvelope => "bad_envelope",
            ExtractError::EmptyChoices => "empty_choices",
            ExtractError::ToolCallsPresent => "tool_calls_present",
            ExtractError::NullContent => "null_content",
            ExtractError::BadContentJson => "invalid_json",
        }
    }
}

/// Extract the candidate JSON value from a 2xx chat-completions body.
/// Pure; the caller schema-validates the result (the local validator is
/// the only judge that counts — server-side `response_format` enforcement
/// is advisory).
pub(crate) fn extract_content(body: &[u8]) -> Result<Value, ExtractError> {
    let envelope: Value = serde_json::from_slice(body).map_err(|_| ExtractError::BadEnvelope)?;
    let choices = envelope
        .get("choices")
        .and_then(Value::as_array)
        .ok_or(ExtractError::BadEnvelope)?;
    let first = choices.first().ok_or(ExtractError::EmptyChoices)?;
    let message = first.get("message").ok_or(ExtractError::BadEnvelope)?;

    let tool_calls_present = message
        .get("tool_calls")
        .is_some_and(|tc| !tc.is_null() && tc.as_array().is_none_or(|a| !a.is_empty()));
    if tool_calls_present {
        return Err(ExtractError::ToolCallsPresent);
    }

    let content = message.get("content").ok_or(ExtractError::NullContent)?;
    if content.is_null() {
        return Err(ExtractError::NullContent);
    }
    let text = content.as_str().ok_or(ExtractError::BadEnvelope)?;
    serde_json::from_str(text.trim()).map_err(|_| ExtractError::BadContentJson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // ---- extract_content: the whole documented failure surface ----

    fn envelope_with_content(content: Value) -> Vec<u8> {
        serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": content } }]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn extract_ok_bare_json_content() {
        let body = envelope_with_content(Value::String("{\"ok\": true}".into()));
        assert_eq!(extract_content(&body), Ok(serde_json::json!({"ok": true})));
    }

    #[test]
    fn extract_trims_whitespace_but_rejects_fences() {
        let ok = envelope_with_content(Value::String("  {\"ok\": true}\n".into()));
        assert_eq!(extract_content(&ok), Ok(serde_json::json!({"ok": true})));
        // A fenced answer is BadContentJson — no fence tolerance here.
        let fenced = envelope_with_content(Value::String("```json\n{\"ok\": true}\n```".into()));
        assert_eq!(extract_content(&fenced), Err(ExtractError::BadContentJson));
    }

    #[test]
    fn extract_body_not_json_is_bad_envelope() {
        assert_eq!(
            extract_content(b"<html>502 Bad Gateway</html>"),
            Err(ExtractError::BadEnvelope)
        );
    }

    #[test]
    fn extract_missing_or_wrong_shape_is_bad_envelope() {
        // No `choices` at all.
        assert_eq!(
            extract_content(br#"{"error": "nope"}"#),
            Err(ExtractError::BadEnvelope)
        );
        // `choices` not an array.
        assert_eq!(
            extract_content(br#"{"choices": "many"}"#),
            Err(ExtractError::BadEnvelope)
        );
        // `message` missing from the first choice.
        assert_eq!(
            extract_content(br#"{"choices": [{"text": "old-style"}]}"#),
            Err(ExtractError::BadEnvelope)
        );
        // `content` present but not a string (e.g. multimodal parts array).
        let body = envelope_with_content(serde_json::json!([{ "type": "text", "text": "{}" }]));
        assert_eq!(extract_content(&body), Err(ExtractError::BadEnvelope));
    }

    #[test]
    fn extract_empty_choices() {
        assert_eq!(
            extract_content(br#"{"choices": []}"#),
            Err(ExtractError::EmptyChoices)
        );
    }

    #[test]
    fn extract_null_or_absent_content() {
        let null_body = envelope_with_content(Value::Null);
        assert_eq!(extract_content(&null_body), Err(ExtractError::NullContent));
        assert_eq!(
            extract_content(br#"{"choices": [{"message": {"role": "assistant"}}]}"#),
            Err(ExtractError::NullContent)
        );
    }

    #[test]
    fn extract_tool_calls_present_rejected_even_with_content() {
        let body = serde_json::json!({
            "choices": [{ "message": {
                "role": "assistant",
                "content": "{\"ok\": true}",
                "tool_calls": [{ "id": "t1", "type": "function",
                                 "function": { "name": "run", "arguments": "{}" } }]
            }}]
        })
        .to_string()
        .into_bytes();
        assert_eq!(extract_content(&body), Err(ExtractError::ToolCallsPresent));
    }

    #[test]
    fn extract_tool_calls_null_or_empty_array_is_not_present() {
        let null_tc = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "{}",
                                       "tool_calls": null } }]
        })
        .to_string()
        .into_bytes();
        assert_eq!(extract_content(&null_tc), Ok(serde_json::json!({})));
        let empty_tc = serde_json::json!({
            "choices": [{ "message": { "role": "assistant", "content": "{}",
                                       "tool_calls": [] } }]
        })
        .to_string()
        .into_bytes();
        assert_eq!(extract_content(&empty_tc), Ok(serde_json::json!({})));
    }

    #[test]
    fn extract_content_not_json_is_bad_content_json() {
        let body = envelope_with_content(Value::String("Sure! Here's the JSON you asked".into()));
        assert_eq!(extract_content(&body), Err(ExtractError::BadContentJson));
    }

    // ---- request_body: the documented wire shape ----

    #[test]
    fn request_body_matches_documented_arli_shape() {
        let schema = serde_json::json!({"type": "object"});
        let text = request_body("say hi", &schema, "some-model");
        let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        assert!(parsed.is_object(), "request body is not JSON: {text}");
        assert_eq!(
            parsed.get("model"),
            Some(&Value::String("some-model".into()))
        );
        assert_eq!(parsed.get("stream"), Some(&Value::Bool(false)));
        assert_eq!(
            parsed.get("tool_choice"),
            Some(&Value::String("none".into()))
        );
        // Documented Arli form: json_schema wraps a bare `schema`, no `name`.
        let rf = parsed.get("response_format");
        assert_eq!(
            rf.and_then(|v| v.get("type")),
            Some(&Value::String("json_schema".into()))
        );
        let js = rf.and_then(|v| v.get("json_schema"));
        assert_eq!(js.and_then(|v| v.get("schema")), Some(&schema));
        assert_eq!(js.and_then(|v| v.get("name")), None);
        // Reasoning explicitly off (Arli defaults it to true).
        assert_eq!(
            parsed.get("include_reasoning"),
            Some(&Value::Bool(false)),
            "include_reasoning: false must be explicit"
        );
        // Prompt rides as the single user message.
        assert_eq!(
            parsed
                .get("messages")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }

    // ---- the HTTP path, against a local plain-HTTP listener ----
    //
    // `call_url` is exercised directly; production code can only reach it
    // through `call`, which pins ENDPOINT. These tests return
    // `io::Result` and FAIL on a bind error: an environment without a
    // loopback socket did not run the transport tests, and that must be a
    // visible failure, never a quiet green.

    /// Serve exactly one connection: capture the full request (headers +
    /// `Content-Length` body), answer with `response`, close.
    fn serve_once(response: String) -> std::io::Result<(String, std::thread::JoinHandle<String>)> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let handle = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return String::new();
            };
            let request = read_http_request(&mut stream);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            request
        });
        Ok((format!("http://{addr}/v1/chat/completions"), handle))
    }

    /// Read one HTTP/1.1 request: until the header terminator, then until
    /// `Content-Length` bytes of body have arrived.
    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut data: Vec<u8> = Vec::new();
        let mut buf = [0u8; 16384];
        while let Ok(n) = stream.read(&mut buf) {
            if n == 0 {
                break;
            }
            data.extend(buf.iter().take(n));
            if let Some(header_end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_text = String::from_utf8_lossy(data.get(..header_end).unwrap_or(&[]));
                let content_length = header_text
                    .lines()
                    .find_map(|l| {
                        let (name, value) = l.split_once(':')?;
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if data.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    fn http_response(status_line: &str, extra_headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status_line}\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn tiny_schema() -> Value {
        serde_json::json!({"type": "object"})
    }

    #[test]
    fn http_200_returns_raw_body_and_sends_key_and_shape() -> std::io::Result<()> {
        let body =
            r#"{"choices": [{"message": {"role": "assistant", "content": "{\"ok\": true}"}}]}"#;
        let (url, handle) = serve_once(http_response(
            "200 OK",
            "Content-Type: application/json\r\n",
            body,
        ))?;
        let outcome = call_url(
            &url,
            "say hi",
            &tiny_schema(),
            "test-model",
            "test-key-123",
            Duration::from_secs(5),
            MAX_RESPONSE_BYTES,
        );
        let got = match outcome {
            ArliOutcome::Http { status, body } => (status, body),
            _ => (0, Vec::new()),
        };
        assert_eq!(got.0, 200, "expected an HTTP 200 outcome");
        assert_eq!(
            String::from_utf8_lossy(&got.1),
            body,
            "raw body must be byte-identical"
        );

        let request = handle.join().unwrap_or_default();
        let request_lower = request.to_ascii_lowercase();
        assert!(
            request_lower.contains("authorization: bearer test-key-123"),
            "Authorization header missing from request: {request}"
        );
        assert!(
            request_lower.contains("content-type: application/json"),
            "Content-Type header missing: {request}"
        );
        assert!(
            request.contains("\"tool_choice\":\"none\""),
            "tool_choice none missing from body: {request}"
        );
        assert!(
            request.contains("\"json_schema\"") && !request.contains("\"name\""),
            "json_schema must use the bare documented form (no name): {request}"
        );
        assert!(
            request.contains("\"include_reasoning\":false"),
            "include_reasoning: false missing from the wire body: {request}"
        );
        Ok(())
    }

    #[test]
    fn http_302_is_returned_not_followed() -> std::io::Result<()> {
        // Location points at a closed port: if ureq followed the redirect,
        // the outcome would be a Transport error, not Http{302} — so this
        // asserts non-following, not just the status passthrough.
        let (url, _handle) = serve_once(http_response(
            "302 Found",
            "Location: http://127.0.0.1:9/\r\n",
            "",
        ))?;
        let outcome = call_url(
            &url,
            "p",
            &tiny_schema(),
            "m",
            "k",
            Duration::from_secs(5),
            MAX_RESPONSE_BYTES,
        );
        let is_302 = matches!(outcome, ArliOutcome::Http { status: 302, .. });
        let refused_as_redirect = matches!(outcome, ArliOutcome::Redirect { .. });
        assert!(
            is_302 || refused_as_redirect,
            "a 3xx must surface as an un-followed redirect outcome"
        );
        Ok(())
    }

    #[test]
    fn response_over_explicit_limit_is_too_large() -> std::io::Result<()> {
        // A 200 whose body exceeds the passed-in bound must classify as
        // TooLarge, not Transport and not a truncated Http success. Small
        // limit here; production passes MAX_RESPONSE_BYTES.
        let big_body = "x".repeat(4096);
        let (url, _handle) = serve_once(http_response(
            "200 OK",
            "Content-Type: application/json\r\n",
            &big_body,
        ))?;
        let outcome = call_url(
            &url,
            "p",
            &tiny_schema(),
            "m",
            "k",
            Duration::from_secs(5),
            64,
        );
        assert!(
            matches!(outcome, ArliOutcome::TooLarge { limit: 64 }),
            "an over-limit body must classify as TooLarge"
        );
        Ok(())
    }

    #[test]
    fn timeout_yields_timed_out() -> std::io::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        // Accept, then stall well past the client timeout without answering.
        let handle = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                std::thread::sleep(Duration::from_millis(1200));
                drop(stream);
            }
        });
        let outcome = call_url(
            &format!("http://{addr}/v1/chat/completions"),
            "p",
            &tiny_schema(),
            "m",
            "k",
            Duration::from_millis(250),
            MAX_RESPONSE_BYTES,
        );
        assert!(
            matches!(outcome, ArliOutcome::TimedOut),
            "a stalled server must classify as TimedOut"
        );
        let _ = handle.join();
        Ok(())
    }

    #[test]
    fn connection_refused_is_transport() -> std::io::Result<()> {
        // Bind to learn a free port, then drop the listener so the connect
        // is refused.
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        drop(listener);
        let outcome = call_url(
            &format!("http://{addr}/v1/chat/completions"),
            "p",
            &tiny_schema(),
            "m",
            "k",
            Duration::from_secs(5),
            MAX_RESPONSE_BYTES,
        );
        assert!(
            matches!(outcome, ArliOutcome::Transport { .. }),
            "a refused connection must classify as Transport"
        );
        Ok(())
    }
}

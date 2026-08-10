//! FD-1.4 structural admission, enforced **during** the parse.
//!
//! # What this replaces, and why replacing it was a prerequisite
//!
//! The first version of this crate's document layer built a complete
//! `serde_json::Value` and *then* walked it for the FD-1.4 depth, array-length
//! and string-length bounds. FD-1.4 asks for a parse-time rejection: "Exceeding
//! any bound is a parse-time rejection, never a truncation." Materialising first
//! satisfied the letter for the byte bound and only the appearance for the
//! structural ones — a document full of small array elements was fully
//! allocated before `ArrayTooLong` fired.
//!
//! At the 1 MiB control-artifact ceiling that overshoot is bounded and
//! unremarkable, which is why it survived step 1 as a documented deferral
//! guarded by a compile-time gate. At the 64 MiB `InteractionManifestV1`
//! ceiling it is not: an array of millions of small numbers costs hundreds of
//! megabytes of `Value` before the bound that exists to refuse it gets a turn.
//! So the gate blocked that type from existing, and this module is what unblocks
//! it — by making the bound true, not by raising the number.
//!
//! # The shape
//!
//! One streaming pass over the bytes, allocating nothing that scales with the
//! document:
//!
//! ```text
//! bytes ──scan──> structural verdict        O(depth) memory, O(bytes) time
//!           │                               depth <= 32 frames, and that is
//!           │                               the entire allocation
//!           ▼
//!        typed deserialization              only after the scan passes
//! ```
//!
//! The scanner holds a stack bounded by [`MAX_JSON_DEPTH`] and nothing else. It
//! never retains a value, a key, or a string — a 64 MiB document with a
//! four-million-element array is refused having allocated at most 32 stack
//! frames.
//!
//! # It is a validator, not a second parser
//!
//! The scan decides *structure*: well-formedness, nesting depth, array length,
//! string length, explicit null, and a top-level object. It never produces a
//! value, so there is no second representation of a document to disagree with
//! the first, and no route by which something reaches a typed schema through
//! this module. Typed deserialization still runs afterwards, over the same
//! bytes, and remains the only thing that produces anything.
//!
//! Being strict here is deliberate: a document this refuses never reaches
//! serde, and a document this accepts is still subject to every typed rule.
//!
//! # Errors never carry payload content
//!
//! Same rule as the rest of the crate (AGENTS.md P0). Paths name positions —
//! `$`, `[3]` — and object members appear as `.*`, never by key. A rejected
//! document is untrusted input and may contain anything.

use crate::bounds::{MAX_ARRAY_LEN, MAX_JSON_DEPTH, MAX_STRING_BYTES};
use crate::json::ParseError;

/// One open container, for path reconstruction and per-array counting.
///
/// An array frame carries the index it is on; an object frame carries nothing,
/// because a member name is untrusted content and never appears in a path.
#[derive(Debug, Clone, Copy)]
enum Frame {
    Array { index: usize },
    Object,
}

/// Whether the scan is deciding admission or merely asking "is this JSON at
/// all".
///
/// The probe exists for one question the top-level rule cannot answer from the
/// first byte: `null` and `not json at all` both start with `n`, so telling a
/// *valid non-object document* (FD-1.3 `NotAnObject`) from *not JSON*
/// (`NotJson`) needs the value actually scanned. In probe mode a null is a
/// value rather than a violation, and a bound that fires is moot — the document
/// is already known not to be a top-level object, which is the verdict either
/// way. Depth stays bounded even here, because the scanner recurses and an
/// unbounded probe would trade a heap bound for a native-stack one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Admit,
    Probe,
}

pub(crate) struct Scanner<'a> {
    input: &'a [u8],
    at: usize,
    line: usize,
    column: usize,
    stack: Vec<Frame>,
    mode: Mode,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            at: 0,
            line: 1,
            column: 1,
            stack: Vec::new(),
            mode: Mode::Admit,
        }
    }

    /// The FD-1.3 top-level rule plus the FD-1.4 structural bounds, in one pass.
    pub(crate) fn run(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        // FD-1.3: "non-object top-level values are rejected at ingest". Which
        // refusal that is depends on whether the document is JSON at all, and
        // the first byte cannot say: `null` and `not json at all` share one.
        if self.peek() != Some(b'{') {
            self.mode = Mode::Probe;
            self.value()?;
            self.skip_whitespace();
            return if self.at < self.input.len() {
                Err(self.syntax())
            } else {
                Err(ParseError::NotAnObject)
            };
        }
        self.value()?;
        self.skip_whitespace();
        if self.at < self.input.len() {
            // Trailing data. serde_json refuses it too; refusing here keeps the
            // verdict in one place rather than splitting it across two layers.
            return Err(self.syntax());
        }
        Ok(())
    }

    fn value(&mut self) -> Result<(), ParseError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(|_| ()),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            // FD-1.3: one uniform null policy. An explicit null is refused
            // wherever it appears, and the path says where without saying what.
            Some(b'n') => {
                self.literal(b"null")?;
                match self.mode {
                    Mode::Admit => Err(ParseError::ExplicitNull { path: self.path() }),
                    Mode::Probe => Ok(()),
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.syntax()),
        }
    }

    fn object(&mut self) -> Result<(), ParseError> {
        self.bump(); // '{'
        self.push(Frame::Object)?;
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.bump();
            self.stack.pop();
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(self.syntax());
            }
            // A member name is scanned and discarded. FD-1.4 bounds "any single
            // string field"; the current admission has always applied that to
            // values, and widening it to keys here would be a bound this
            // implementation invented rather than one the contract states.
            self.string_unbounded()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(self.syntax());
            }
            self.bump();
            self.value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                }
                Some(b'}') => {
                    self.bump();
                    self.stack.pop();
                    return Ok(());
                }
                _ => return Err(self.syntax()),
            }
        }
    }

    fn array(&mut self) -> Result<(), ParseError> {
        self.bump(); // '['
        self.push(Frame::Array { index: 0 })?;
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.bump();
            self.stack.pop();
            return Ok(());
        }
        let mut count: usize = 0;
        loop {
            // FD-1.4, and the whole point of scanning: the cap is checked as
            // the element is reached, so entry 4097 is refused having allocated
            // nothing for the 4096 before it.
            count = count.saturating_add(1);
            if count > MAX_ARRAY_LEN && self.mode == Mode::Admit {
                let path = self.enclosing_path();
                return Err(ParseError::ArrayTooLong {
                    path,
                    actual: count,
                });
            }
            if let Some(Frame::Array { index }) = self.stack.last_mut() {
                *index = count - 1;
            }
            self.value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.bump();
                }
                Some(b']') => {
                    self.bump();
                    self.stack.pop();
                    return Ok(());
                }
                _ => return Err(self.syntax()),
            }
        }
    }

    /// A string value, bounded by FD-1.4.
    fn string(&mut self) -> Result<usize, ParseError> {
        let len = self.string_unbounded()?;
        if len > MAX_STRING_BYTES && self.mode == Mode::Admit {
            return Err(ParseError::StringTooLong {
                path: self.path(),
                actual: len,
            });
        }
        Ok(len)
    }

    /// Scan a JSON string and return its byte length, without retaining it.
    ///
    /// The length counts the encoded bytes between the quotes, which is what
    /// the previous `Value`-based walk measured for an unescaped string and the
    /// closest honest equivalent for an escaped one — and it is an upper bound
    /// on the decoded length, so it never admits something the bound refuses.
    fn string_unbounded(&mut self) -> Result<usize, ParseError> {
        self.bump(); // opening quote
        let start = self.at;
        loop {
            match self.peek() {
                None => return Err(self.syntax()),
                Some(b'"') => {
                    let len = self.at - start;
                    self.bump();
                    return Ok(len);
                }
                Some(b'\\') => {
                    self.bump();
                    // RFC 8259 fixes the escape set. Accepting anything after a
                    // backslash would let this layer admit a string serde then
                    // refuses, which splits the verdict across two layers —
                    // the failure mode the equivalence tests exist to catch.
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.bump();
                        }
                        Some(b'u') => {
                            self.bump();
                            for _ in 0..4 {
                                match self.peek() {
                                    Some(c) if c.is_ascii_hexdigit() => self.bump(),
                                    _ => return Err(self.syntax()),
                                }
                            }
                        }
                        _ => return Err(self.syntax()),
                    }
                }
                // A raw control byte is invalid JSON; serde refuses it and so
                // does this, so the two agree on what a string is.
                Some(c) if c < 0x20 => return Err(self.syntax()),
                Some(_) => self.bump(),
            }
        }
    }

    fn number(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        // RFC 8259: `int = zero / (digit1-9 *DIGIT)`. A leading zero followed
        // by a digit is not a number, and serde refuses it — so this must too,
        // or the two layers disagree about what the document even is.
        if self.peek() == Some(b'0') {
            self.bump();
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.syntax());
            }
        } else {
            self.digits()?;
        }
        if self.peek() == Some(b'.') {
            self.bump();
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            self.digits()?;
        }
        if self.at == start {
            return Err(self.syntax());
        }
        Ok(())
    }

    fn digits(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        if self.at == start {
            return Err(self.syntax());
        }
        Ok(())
    }

    fn literal(&mut self, word: &[u8]) -> Result<(), ParseError> {
        for expected in word {
            if self.peek() != Some(*expected) {
                return Err(self.syntax());
            }
            self.bump();
        }
        Ok(())
    }

    fn push(&mut self, frame: Frame) -> Result<(), ParseError> {
        // Checked before pushing, so the stack itself is the bound: a document
        // nested a million deep costs 32 frames and a refusal, not a million
        // frames or a blown native stack.
        if self.stack.len() >= MAX_JSON_DEPTH {
            // Bounded in both modes: the scanner recurses, so an unbounded
            // probe would trade a heap bound for a native-stack one. In probe
            // mode the document is already known not to be a top-level object,
            // so `NotAnObject` is the verdict regardless of what lies deeper.
            return Err(match self.mode {
                Mode::Admit => ParseError::DepthExceeded { path: self.path() },
                Mode::Probe => ParseError::NotAnObject,
            });
        }
        self.stack.push(frame);
        Ok(())
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }

    fn bump(&mut self) {
        if self.peek() == Some(b'\n') {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        self.at += 1;
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.bump();
        }
    }

    fn syntax(&self) -> ParseError {
        ParseError::NotJson {
            category: "syntax",
            line: self.line,
            column: self.column,
        }
    }

    /// The position of the value being scanned. Array indices are positions and
    /// safe to name; object members are content and appear only as `.*`.
    fn path(&self) -> String {
        let mut path = String::from("$");
        for frame in &self.stack {
            match frame {
                Frame::Array { index } => path.push_str(&format!("[{index}]")),
                Frame::Object => path.push_str(".*"),
            }
        }
        path
    }

    /// The path of the container currently being scanned, for a bound that is a
    /// property of the container rather than of an element.
    fn enclosing_path(&self) -> String {
        self.path()
    }
}

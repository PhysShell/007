"""B1 acceptance: the Rust `hf:` meter counts identically to Python `tokenizers`.

Level 2 is only honest if qodec chooses aliases and accepts codecs under the
tokenizer the served model actually reads. This proves the wired Rust meter
(`qodec encode --meter hf:<tokenizer.json>`, reading tokens_in) agrees
bit-for-bit with the reference `tokenizers` library on a golden corpus.

Skips unless both the `tokenizers` Python package and a tokenizer.json are
available. The tokenizer is found via QODEC_HF_TOKENIZER, else the bench cache.
"""

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import qodec  # noqa: E402

# Representative agent-payload strings: code+path, symbols, JSON, CJK, prose.
GOLDEN = [
    "src/main.rs:12: warning: unused variable `parser`",
    "fn parse_derive(cmd: &Command, matches: &ArgMatches) -> Result<T, Error>",
    'clap::builder::Arg::new("verbose").short(\'v\').long("verbose")',
    '{"file":"src/lib.rs","symbol":"Parser::parse","line":42}',
    "警告: 引数の解析に失敗しました（derive マクロ）",
    "The quick brown fox jumps over the lazy dog, then parses arguments.",
    "".join(f"impl Parser for Cli{i} {{}}\n" for i in range(20)),
    "",
    "\n\n   \t  \n",
]


def _tokenizer_path() -> str | None:
    env = os.environ.get("QODEC_HF_TOKENIZER")
    if env and Path(env).exists():
        return env
    cache = Path.home() / ".cache" / "qodec-bench" / "tokenizer.json"
    if cache.exists():
        return str(cache)
    return None


def _py_tokenizers():
    try:
        import tokenizers  # noqa: F401
        return tokenizers
    except Exception:
        return None


_TOK_PATH = _tokenizer_path()
_PY = _py_tokenizers()
try:
    qodec.binary()
    _QODEC = True
except Exception:
    _QODEC = False


@unittest.skipUnless(_TOK_PATH and _PY and _QODEC,
                     "needs tokenizers + a tokenizer.json (QODEC_HF_TOKENIZER) + qodec")
class HfMeterParity(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ref = _PY.Tokenizer.from_file(_TOK_PATH)
        cls.meter = f"hf:{_TOK_PATH}"

    def _rust_count(self, text: str) -> int:
        out = subprocess.run(
            [str(qodec.binary()), "encode", "--codec", "fold", "--meter", self.meter, "--json"],
            input=text, capture_output=True, text=True, check=True,
        )
        return json.loads(out.stdout)["tokens_in"]

    def test_counts_match_reference_on_golden_corpus(self):
        for text in GOLDEN:
            py = len(self.ref.encode(text, add_special_tokens=False).ids)
            rust = self._rust_count(text)
            self.assertEqual(rust, py, f"mismatch on {text!r}: rust={rust} py={py}")

    def test_meter_name_records_the_tokenizer(self):
        # The run must record which tokenizer produced the numbers.
        env = qodec.encode("Parser::parse\n" * 10, meter=self.meter, passthrough=True)
        self.assertEqual(env.meter, self.meter)


if __name__ == "__main__":
    unittest.main()

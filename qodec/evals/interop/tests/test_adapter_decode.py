"""decode_envelope respects the `encoded` flag.

The trap: a passthrough (encoded=false) whose content is itself container-shaped
(a literal `%q1 …` string in tool output). Running it through `qodec decode`
would unwrap a container the codec never made. `decode_envelope` must return it
byte-identical instead.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import qodec  # noqa: E402


def _qodec_present() -> bool:
    try:
        qodec.binary()
        return True
    except Exception:
        return False


# A valid, container-shaped payload that is ALSO unique enough that qodec finds
# no gain and passes it through verbatim.
_CONTAINER_SHAPED = "%q1 raw\n%q1 body\nthe quick brown fox jumps over the lazy dog once\n"


@unittest.skipUnless(_qodec_present(), "qodec binary not built")
class DecodeEnvelope(unittest.TestCase):
    def test_container_shaped_passthrough_roundtrips_byte_exact(self):
        env = qodec.encode(_CONTAINER_SHAPED, passthrough=True)
        self.assertFalse(env.encoded, "container-shaped unique text should pass through")
        back, ms = qodec.decode_envelope(env)
        self.assertEqual(back, _CONTAINER_SHAPED, "passthrough must return input byte-identical")
        self.assertEqual(ms, 0.0, "passthrough decode does no work")

    def test_plain_decode_would_have_unwrapped_it(self):
        # Proves decode_envelope is doing something: raw `decode` unwraps the
        # container-shaped text, which is exactly the corruption we avoid.
        env = qodec.encode(_CONTAINER_SHAPED, passthrough=True)
        wrongly, _ = qodec.decode(env.content)
        self.assertNotEqual(wrongly, _CONTAINER_SHAPED)
        self.assertIn("the quick brown fox", wrongly)  # body got unwrapped

    def test_real_artifact_still_decodes(self):
        repetitive = "".join(f"src/mod{i%3}/file.rs:{i}: warning: unused import\n" for i in range(40))
        env = qodec.encode(repetitive, passthrough=True)
        self.assertTrue(env.encoded, "repetitive text should mine")
        back, ms = qodec.decode_envelope(env)
        self.assertEqual(back, repetitive)
        self.assertGreater(ms, 0.0)


if __name__ == "__main__":
    unittest.main()

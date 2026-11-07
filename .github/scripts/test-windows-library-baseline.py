"""Focused regression checks for the C5c comparison."""
import runpy
import unittest
from pathlib import Path

module = runpy.run_path(str(Path(__file__).with_name("windows-library-baseline.py")))
compare = module["compare"]


def transcript(compiler_pass=False):
    expected = module["EXPECTED"].copy()
    compiler = module["COMPILER"]
    if compiler_pass:
        del expected[compiler]
    sections = []
    for name, reason in expected.items():
        if name == module["COMPILER"]:
            reason += "\n   --> C:\\Temp\\journal_content.rs:242:1\n242 | mod tests;\n\nerror: aborting due to 1 previous error"
        sections.append(f"---- {name} stdout ----\n\nthread '{name}' (1) panicked at src\\fixture.rs:1:1:\n{reason}\n\n")
    return (
        (f"test {compiler} ... ok\n" if compiler_pass else "")
        + "".join(f"test {name} ... FAILED\n" for name in expected)
        + "".join(f"test {name} ... ignored\n" for name in module["IGNORED"])
        + "".join(sections)
        + f"test result: FAILED. 377 passed; {len(expected)} failed; 2 ignored; 0 measured; 0 filtered out;\n"
    )


class BaselineTest(unittest.TestCase):
    def test_matching_baseline(self):
        compare(transcript(), 101)

    def test_compiler_transition_requires_pass_and_exact_remaining_failures(self):
        corrected = transcript(compiler_pass=True)
        compare(corrected, 101, require_compiler_pass=True)
        for rejected in [
            transcript(),
            corrected.replace(f"test {module['COMPILER']} ... ok\n", ""),
            corrected.replace("left: Timeout", "left: ServiceUnavailable"),
            "test unexpected ... FAILED\n" + corrected,
        ]:
            with self.assertRaises(ValueError):
                compare(rejected, 101, require_compiler_pass=True)

    def test_rejects_changes(self):
        original = transcript()
        for changed, status in [
            (original, 1),
            (original.replace("left: Timeout", "left: ServiceUnavailable"), 101),
            (original.replace("E0583", "E0425"), 101),
            (original.replace("database_migration_platform_unsupported", "database_corrupt"), 101),
            (original.replace("0 filtered out", "1 filtered out"), 101),
            ("test unexpected ... FAILED\n" + original, 101),
            ("test unexpected ... ignored\n" + original, 101),
            (original.replace(f"test {module['NETWORK']} ... FAILED", "test changed_identity ... FAILED"), 101),
        ]:
            with self.subTest(status=status, changed=changed != original):
                with self.assertRaises(ValueError):
                    compare(changed, status)


if __name__ == "__main__":
    unittest.main()

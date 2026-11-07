"""Check the C5c baseline from Actions run 41894600284."""
import argparse
import re
import sys
from pathlib import Path

COMPILER = "backup::journal_content::tests::compiler_rejects_sql_access_and_snapshot_replacement"
NETWORK = "weather::historical::tests::refused_connection_remains_a_network_failure"
MIGRATION = 'called `Result::unwrap()` on an `Err` value: "database_migration_platform_unsupported"'
EXPECTED = {
    COMPILER: 'error[E0583]: file not found for module `tests`',
    NETWORK: 'assertion `left == right` failed\n  left: Timeout\n right: Network',
    "weather::storage::legacy_tests::legacy_recovery_reads_absent_weather_and_requires_safe_startup_before_write": MIGRATION,
    "weather::view::tests::legacy_reads_do_not_migrate_and_corrupt_weather_is_not_presented": MIGRATION,
}
IGNORED = {
    "photographs::tests::preparation_measurements",
    "weather::historical::recent::tests::live_forecast_contract_synthetic_coordinates",
}


def compare(output, status, require_compiler_pass=False):
    expected = EXPECTED.copy()
    if require_compiler_pass:
        del expected[COMPILER]
    output = re.sub(r"\x1b\[[0-9;]*m", "", output).replace("\r\n", "\n")
    if require_compiler_pass and re.findall(
        rf"^test {re.escape(COMPILER)} \.\.\. (.*)$", output, re.M
    ) != ["ok"]:
        raise ValueError("Expected exactly one explicit pass for the compiler safety test")
    failed = re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.M)
    ignored = re.findall(r"^test (\S+) \.\.\. ignored(?:, .*)?$", output, re.M)
    summaries = re.findall(r"^test result: FAILED\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output, re.M)
    if status != 101 or len(summaries) != 1 or summaries[0][1:] != (str(len(expected)), "2", "0", "0"):
        raise ValueError(f"Expected a completed, unfiltered library test run with {len(expected)} failures and two ignored")
    if int(summaries[0][0]) == 0 or sorted(failed) != sorted(expected) or sorted(ignored) != sorted(IGNORED):
        raise ValueError(f"Unexpected failed or ignored tests: {failed}, {ignored}")

    sections = re.split(r"^---- (\S+) stdout ----\n", output, flags=re.M)
    if sorted(sections[1::2]) != sorted(expected):
        raise ValueError("Missing, duplicate or unexpected failure diagnostics")
    for name, section in zip(sections[1::2], sections[2::2]):
        panic = re.search(rf"^thread '{re.escape(name)}' \(\d+\) panicked at [^\n]+:\n", section, re.M)
        if not panic:
            raise ValueError(f"Missing panic diagnostic for {name}")
        payload = section[panic.end():].split("\nfailures:", 1)[0].rstrip()
        reason = payload.split("\n\n", 1)[0]
        if name == COMPILER:
            errors = re.findall(r"^error(?:\[\w+\])?:.*$", payload, re.M)
            matches = (
                errors == [expected[name], "error: aborting due to 1 previous error"]
                and re.search(r"^\s*--> .*[/\\]journal_content\.rs:\d+:\d+$", reason, re.M)
                and re.search(r"^\d+ \| mod tests;$", reason, re.M)
            )
        else:
            matches = reason == expected[name]
        if not matches:
            raise ValueError(f"Failure reason changed for {name}: {reason}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("status", type=int)
    parser.add_argument("--require-compiler-pass", action="store_true")
    args = parser.parse_args()
    try:
        compare(args.log.read_text(encoding="utf-8", errors="replace"), args.status, args.require_compiler_pass)
    except ValueError as error:
        sys.exit(str(error))
    if args.require_compiler_pass:
        print("Compiler safety test passed. The other three C5c failures and two ignores matched.")
    else:
        print("Matched the four C5c failure identities and reasons and the two approved ignores.")

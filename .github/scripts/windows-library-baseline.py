"""Check the C5c baseline from Actions run 41894600284."""
import argparse
import re
import sys
from pathlib import Path

COMPILER = "backup::journal_content::tests::compiler_rejects_sql_access_and_snapshot_replacement"
NETWORK = "weather::historical::tests::refused_connection_remains_a_network_failure"
NETWORK_REPLACEMENT = "weather::historical::tests::disconnected_http_peer_remains_a_network_failure"
MIGRATION = 'called `Result::unwrap()` on an `Err` value: "database_migration_platform_unsupported"'
LEGACY_WEATHER = (
    "weather::storage::legacy_tests::legacy_recovery_reads_absent_weather_and_requires_safe_startup_before_write",
    "weather::view::tests::legacy_reads_do_not_migrate_and_corrupt_weather_is_not_presented",
)
EXPECTED = {
    COMPILER: 'error[E0583]: file not found for module `tests`',
    NETWORK: 'assertion `left == right` failed\n  left: Timeout\n right: Network',
    **dict.fromkeys(LEGACY_WEATHER, MIGRATION),
}
IGNORED = {
    "photographs::tests::preparation_measurements",
    "weather::historical::recent::tests::live_forecast_contract_synthetic_coordinates",
}


def compare(output, status, require_compiler_pass=False, require_legacy_weather_pass=False, require_clean=False):
    expected = EXPECTED.copy()
    required = (COMPILER,) if require_compiler_pass else ()
    if require_legacy_weather_pass:
        required += LEGACY_WEATHER
    if require_clean:
        required = (COMPILER, *LEGACY_WEATHER, NETWORK_REPLACEMENT)
        expected.clear()
    output = re.sub(r"\x1b\[[0-9;]*m", "", output).replace("\r\n", "\n")
    for name in required:
        expected.pop(name, None)
        if re.findall(rf"^test {re.escape(name)} \.\.\. (.*)$", output, re.M) != ["ok"]:
            raise ValueError(f"Expected exactly one explicit pass for {name}")
    failed = re.findall(r"^test (\S+) \.\.\. FAILED$", output, re.M)
    ignored = re.findall(r"^test (\S+) \.\.\. ignored(?:, .*)?$", output, re.M)
    summaries = re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", output, re.M)
    verdict = "ok" if require_clean else "FAILED"
    # Child-process probes can print their own filtered summaries.
    summaries = [summary for summary in summaries if summary[-1] == "0"]
    if status != (0 if require_clean else 101) or len(summaries) != 1 or summaries[0][0] != verdict or summaries[0][2:] != (str(len(expected)), "2", "0", "0"):
        raise ValueError(f"Expected a completed, unfiltered library test run with {len(expected)} failures and two ignored")
    if int(summaries[0][1]) == 0 or sorted(failed) != sorted(expected) or sorted(ignored) != sorted(IGNORED):
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
    parser.add_argument("--require-legacy-weather-pass", action="store_true")
    parser.add_argument("--require-clean", action="store_true")
    args = parser.parse_args()
    try:
        compare(args.log.read_text(encoding="utf-8", errors="replace"), args.status, args.require_compiler_pass, args.require_legacy_weather_pass, args.require_clean)
    except ValueError as error:
        sys.exit(str(error))
    if args.require_clean:
        print("All four regression tests passed. No library failures and only the two approved ignores.")
    elif args.require_legacy_weather_pass:
        print("Both legacy weather tests passed and the remaining baseline matched.")
    elif args.require_compiler_pass:
        print("Compiler safety test passed. The other three C5c failures and two ignores matched.")
    else:
        print("Matched the four C5c failure identities and reasons and the two approved ignores.")

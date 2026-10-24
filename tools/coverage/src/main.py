import argparse
import concurrent.futures
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import covdata as profiles
import report
import runner


def gather_sources(paths):
    out = []
    for path in paths:
        if os.path.isdir(path):
            for name in sorted(os.listdir(path)):
                if name.endswith(".fi"):
                    out.append(os.path.join(path, name))
        else:
            out.append(path)
    return out


MAIN_PATTERN = re.compile(r"^\s*(export\s+)?fn\s+main\b", re.M)
TEST_PATTERN = re.compile(r"^\s*#\[test\]", re.M)


def plan_source(source, test_mode):
    if test_mode:
        return "test"
    try:
        with open(source, encoding="utf-8", errors="replace") as handle:
            text = handle.read()
    except OSError:
        return "run"
    if MAIN_PATTERN.search(text):
        return "run"
    if TEST_PATTERN.search(text):
        return "test"
    return "skip"


def add_run_options(sub):
    sub.add_argument("--opt", default="dev", choices=["dev", "dev-fast", "release-safe", "release-fast"])
    sub.add_argument("--test", action="store_true")
    sub.add_argument("--timeout", type=int, default=900)
    sub.add_argument("-o", "--out", required=True)


def add_filter_options(sub):
    sub.add_argument("--include", action="append", default=[])
    sub.add_argument("--exclude", action="append", default=[])


def run_command(args):
    sources = gather_sources(args.sources)
    if not sources:
        raise runner.RunError("no .fi sources given")
    if args.command == "run" and len(sources) != 1:
        raise runner.RunError("run takes exactly one source")
    program_args = args.program_args if args.command == "run" else []

    skipped = []
    failed = []

    def one(source):
        if args.command == "run":
            return runner.run_one(source, args.opt, args.test, program_args, args.timeout)
        mode = plan_source(source, args.test)
        if mode == "skip":
            skipped.append(source)
            return None
        try:
            return runner.run_one(source, args.opt, mode == "test", program_args, args.timeout)
        except runner.RunError as error:
            failed.append((source, str(error).splitlines()[0]))
            return None

    result = profiles.empty()
    merged = 0
    jobs = getattr(args, "jobs", 1)
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        for item in pool.map(one, sources):
            if item is not None:
                profiles.merge(result, item)
                merged += 1
    if args.command == "suite":
        print(f"suite: {merged} profiled, {len(skipped)} skipped (no main, no tests), {len(failed)} failed", file=sys.stderr)
        for source in skipped:
            print(f"  skipped {source}", file=sys.stderr)
        for source, reason in failed:
            print(f"  failed {reason}", file=sys.stderr)
        if merged == 0:
            raise runner.RunError("suite produced no profile")
    profiles.save(result, args.out)
    print(report.text(result), end="")
    return 0


def merge_command(args):
    result = profiles.empty()
    for path in args.profiles:
        profiles.merge(result, profiles.load(path))
    profiles.save(result, args.out)
    return 0


def report_command(args):
    result = profiles.empty()
    for path in args.profiles:
        profiles.merge(result, profiles.load(path))
    if args.format == "text":
        sys.stdout.write(report.text(result, args.include, args.exclude, args.missing))
    elif args.format == "lcov":
        sys.stdout.write(report.lcov(result, args.include, args.exclude))
    elif args.format == "json":
        import json
        json.dump(report.totals(result, args.include, args.exclude), sys.stdout, indent=1)
        sys.stdout.write("\n")
    else:
        sys.stdout.write(report.annotate(result, args.annotate, args.root))
    if args.fail_under is not None:
        if report.totals(result, args.include, args.exclude)["line_percent"] < args.fail_under:
            return 1
    return 0


def build_parser():
    parser = argparse.ArgumentParser(prog="firncov")
    commands = parser.add_subparsers(dest="command", required=True)
    one = commands.add_parser("run")
    add_run_options(one)
    one.add_argument("sources", nargs=1)
    one.add_argument("program_args", nargs="*")
    many = commands.add_parser("suite")
    add_run_options(many)
    many.add_argument("-j", "--jobs", type=int, default=2)
    many.add_argument("sources", nargs="+")
    joined = commands.add_parser("merge")
    joined.add_argument("-o", "--out", required=True)
    joined.add_argument("profiles", nargs="+")
    shown = commands.add_parser("report")
    add_filter_options(shown)
    shown.add_argument("--format", default="text", choices=["text", "lcov", "json", "annotate"])
    shown.add_argument("--missing", action="store_true")
    shown.add_argument("--annotate")
    shown.add_argument("--root", default=".")
    shown.add_argument("--fail-under", type=float)
    shown.add_argument("profiles", nargs="+")
    return parser


def main(argv):
    args = build_parser().parse_args(argv)
    try:
        if args.command in ("run", "suite"):
            return run_command(args)
        if args.command == "merge":
            return merge_command(args)
        if args.format == "annotate" and not args.annotate:
            raise runner.RunError("--format annotate needs --annotate FILE")
        return report_command(args)
    except (runner.RunError, ValueError, KeyError, OSError) as error:
        print(f"firncov: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

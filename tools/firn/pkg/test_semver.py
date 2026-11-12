import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import semver

failures = []


def expect(requirement, version, wanted):
    got = semver.satisfies(requirement, version)
    if got != wanted:
        failures.append("%s vs %s: wanted %s" % (requirement, version, wanted))


def expect_resolve(requirement, versions, wanted):
    got = semver.resolve(requirement, versions)
    if got != wanted:
        failures.append("resolve %s: wanted %s got %s" % (requirement, wanted, got))


CASES = [
    ("1.2.3", "1.2.3", True), ("1.2.3", "1.9.0", True), ("1.2.3", "2.0.0", False), ("1.2.3", "1.2.2", False),
    ("^1.2", "1.2.0", True), ("^1.2", "1.99.9", True), ("^1.2", "2.0.0", False), ("^1", "1.0.0", True),
    ("^0.2.3", "0.2.9", True), ("^0.2.3", "0.3.0", False), ("^0.2.3", "0.2.2", False),
    ("^0.0.3", "0.0.3", True), ("^0.0.3", "0.0.4", False), ("^0", "0.9.9", True), ("^0", "1.0.0", False),
    ("~1.2.3", "1.2.9", True), ("~1.2.3", "1.3.0", False), ("~1.2", "1.2.0", True), ("~1", "1.9.9", True), ("~1", "2.0.0", False),
    ("=1.2.3", "1.2.3", True), ("=1.2.3", "1.2.4", False), ("=1.2", "1.2.7", True), ("=1.2", "1.3.0", False),
    (">=1.2.0", "3.0.0", True), (">=1.2.0", "1.1.9", False), (">1.2.0", "1.2.0", False), (">1.2.0", "1.2.1", True),
    ("<2.0.0", "1.99.99", True), ("<2.0.0", "2.0.0", False), ("<=1.2", "1.2.9", True), ("<=1.2", "1.3.0", False),
    (">=1.2, <1.5", "1.4.9", True), (">=1.2, <1.5", "1.5.0", False), (">= 1.2.0 < 2", "1.9.0", True),
    ("*", "0.0.1", True), ("1.*", "1.7.0", True), ("1.*", "2.0.0", False), ("1.2.*", "1.2.5", True), ("1.2.*", "1.3.0", False),
    ("^1.2.3-beta.1", "1.2.3-beta.2", True), ("^1.2.3-beta.1", "1.2.3-alpha", False), ("^1.2.3-beta.1", "1.2.3", True),
    ("^1.2.3", "1.2.4-beta", False), ("*", "1.0.0-rc.1", False), ("^1.2.3", "1.2.3+build5", True),
]
for case in CASES:
    expect(*case)

expect_resolve("^1.2", ["1.0.0", "1.2.0", "1.10.0", "1.9.0", "2.0.0"], "1.10.0")
expect_resolve("^2", ["1.0.0"], None)
expect_resolve("*", ["1.0.0-rc.1", "0.9.0"], "0.9.0")
expect_resolve("^1.0.0-rc.1", ["1.0.0-rc.1", "1.0.0-rc.2", "1.0.0-beta"], "1.0.0-rc.2")
expect_resolve("^1", ["1.0.0-rc.2", "1.0.0"], "1.0.0")

ORDER = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"]
keys = [semver.sort_key(semver.parse_version(v)) for v in ORDER]
if keys != sorted(keys) or len(set(keys)) != len(keys):
    failures.append("precedence order of the semver.org example")

for bad in ("1.2", "x.y.z", "1.2.3.4", ""):
    try:
        semver.parse_version(bad)
        failures.append("accepted bad version " + repr(bad))
    except semver.SemverError:
        pass
for bad in ("", "^^1", "a.b"):
    try:
        semver.parse_requirement(bad)
        failures.append("accepted bad requirement " + repr(bad))
    except semver.SemverError:
        pass

for failure in failures:
    print("FAIL " + failure)
print("%d semver checks, %d failure(s)" % (len(CASES) + 5 + 1 + 7, len(failures)))
sys.exit(1 if failures else 0)

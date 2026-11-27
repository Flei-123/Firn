let failures = 0;
let total = 0;

export function check(name, actual, expected) {
  total += 1;
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (!ok) {
    failures += 1;
    console.log(`FAIL ${name}\n  expected ${JSON.stringify(expected)}\n  actual   ${JSON.stringify(actual)}`);
  } else {
    console.log(`PASS ${name}`);
  }
}

export function finish() {
  console.log(`${failures} failed of ${total}`);
  process.exit(failures ? 1 : 0);
}

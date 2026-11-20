# 7. Building and testing

```firn test
import std.testkit

fn add(a: i32, b: i32) -> i32 {
    return a + b
}

#[test]
fn adds_numbers() {
    testkit.assert_eq[i32](add(2, 3), 5)
}

#[test]
fn compares_text() {
    testkit.assert_eq_str("fi" + "rn", "firn")
}
```

- `firn build DIR -o app` builds a package; `firn build x.fi` a single file.
- `firn test` runs the `tests/*.fi` of a package (TAP output).
- `firn check` type-checks in a fraction of a second; `firn fmt -c` checks formatting; `firn doc .` writes the API.
- Build levels: `--opt-level=dev|dev-fast|release-safe|release-fast`; checked arithmetic stays on except in `release-fast`.
- Targets: `firn new desktop-app gui`, then `firn build --target web|android|windows|linux`.

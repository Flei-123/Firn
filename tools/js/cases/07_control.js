// Statements, exceptions, labels.
var out = [];
for (var i = 0; i < 3; i++) { for (var j = 0; j < 3; j++) { if (j === 1) continue; if (i === 2) break; out.push(i + "" + j); } }
print(out.join(","));
outer: for (var k = 0; k < 3; k++) { for (var m = 0; m < 3; m++) { if (m === 1) continue outer; out.push("x" + k + m); } }
print(out.join(","));
var s = 0, n = 5;
do { s += n; } while (--n > 0);
print(s);
switch (2) { case 1: print("one"); case 2: print("two"); case 3: print("three"); break; default: print("d"); }
try { null.x; } catch (err) { print(err instanceof TypeError, err.name); } finally { print("finally"); }
function f() { try { return "try"; } finally { print("cleanup"); } }
print(f());
try { throw { code: 42 }; } catch (e) { print(e.code); }
var obj = { a: 1, b: 2 };
var keys = []; for (var key in obj) { keys.push(key); }
print(keys.join(","));
for (const v of [10, 20]) { print(v); }

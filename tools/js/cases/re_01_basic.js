var pats = ["a+", "a*b", "^x", "y$", ".", "[abc]", "[^abc]", "[a-z]+", "\\d+",
            "\\w\\W", "\\s+", "a{2,3}", "a+?", "(?:ab)+", "a|bc", "\\bfoo\\b",
            "(?=x)", "(?!x).", "[\\d-a]", "\\u0041", "\\x42", "\\n", "a\\/b"];
var subj = ["", "a", "aa", "aaa", "abc", "xabcx", "a b", "foo bar", "A1B2",
            "  ", "a/b", "\nx", "ab-ab", "zzz"];
for (var i = 0; i < pats.length; i++) {
  for (var j = 0; j < subj.length; j++) {
    var re;
    try { re = new RegExp(pats[i]); } catch (e) { print("ERR", pats[i]); break; }
    var m = re.exec(subj[j]);
    print(pats[i], JSON.stringify(subj[j]), m === null ? "null" : m.index + ":" + JSON.stringify(m[0]));
  }
}
var flags = ["", "i", "g", "m", "s", "gi", "gm"];
for (var f = 0; f < flags.length; f++) {
  var r2 = new RegExp("A.b", flags[f]);
  print(flags[f], r2.source, r2.flags, r2.global, r2.ignoreCase, r2.multiline, r2.dotAll,
        r2.test("xa\nbz"), r2.test("xAqbz"));
}

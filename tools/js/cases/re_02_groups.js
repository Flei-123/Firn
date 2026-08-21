var cases = [
  ["(a)(b)?(c)", "ac"], ["(a)(b)?(c)", "abc"], ["(a+)(a+)", "aaaa"],
  ["(a|b)+", "abab"], ["((a)(b))", "ab"], ["(?<x>a)(?<y>b)", "ab"],
  ["(a)\\1", "aa"], ["(a)\\1", "ab"], ["(?<z>a)\\k<z>", "aa"],
  ["(a*)*", "aaa"], ["(a?)*b", "b"], ["(?:(a)|(b))+", "ab"],
  ["^(?:(a)|b)*$", "aba"], ["(a)(?=b)", "ab"], ["(?<=(a))b", "ab"],
  ["(?<!x)(a)", "ba"], ["(a){0}", "a"], ["(a){2,}", "aaaa"],
  ["([a-c])([d-f])?", "ad"], ["([a-c])([d-f])?", "a"]
];
for (var i = 0; i < cases.length; i++) {
  var re = new RegExp(cases[i][0]);
  var m = re.exec(cases[i][1]);
  if (m === null) { print(cases[i][0], cases[i][1], "null"); continue; }
  var parts = [];
  for (var k = 0; k < m.length; k++) parts.push(m[k] === undefined ? "u" : JSON.stringify(m[k]));
  print(cases[i][0], cases[i][1], m.index, parts.join(","),
        m.groups === undefined ? "-" : JSON.stringify(m.groups));
}
var g = /(\d)(\w)/g, s = "1a 2b 3c", out = [], mm;
while ((mm = g.exec(s)) !== null) out.push(mm.index + ":" + mm[1] + mm[2] + ":" + g.lastIndex);
print(out.join(" "));
print(JSON.stringify("1a2b".match(/(\d)(\w)/g)));
print(JSON.stringify([..."1a2b".matchAll(/(\d)(\w)/g)].map(function(x){return x.index+x[2];})));

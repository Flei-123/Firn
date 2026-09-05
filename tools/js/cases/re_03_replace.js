var pairs = [
  ["abc", /b/, "X"], ["abc", /b/g, "X"], ["aaa", /a/g, "$&$&"],
  ["abc", /b/, "[$`|$']"], ["abc", /(b)/, "<$1>"], ["abc", /(b)/, "<$2>"],
  ["a-b", /-/, "$$"], ["2020-05", /(?<y>\d+)-(?<m>\d+)/, "$<m>.$<y>"],
  ["xyz", /q/, "N"], ["aaa", /a*/g, "-"], ["abc", /(a)(b)(c)/, "$3$2$1"],
  ["abc", /b/, function (m, i, s) { return "[" + m + i + s.length + "]"; }],
  ["a1b2", /(\w)(\d)/g, function (m, a, b, i) { return b + a + i; }]
];
for (var i = 0; i < pairs.length; i++) {
  print(JSON.stringify(pairs[i][0].replace(pairs[i][1], pairs[i][2])));
}
print(JSON.stringify("a-b-c".replaceAll("-", "+")));
print(JSON.stringify("aaa".replaceAll(/a/g, "b")));
print(JSON.stringify("abc".replace("b", "$&$&")));
print(JSON.stringify("".replace(/^/, "X")));
print(JSON.stringify("abc".replace(/(?:)/g, "-")));

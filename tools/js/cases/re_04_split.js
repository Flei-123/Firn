var cases = [
  ["a,b,c", /,/], ["a,b,c", ","], ["abc", ""], ["abc", /(?:)/],
  ["a1b2c", /\d/], ["a1b2c", /(\d)/], ["", /a/], ["", ""],
  ["aaa", /a/], ["axbxc", /x/], ["a-b_c", /[-_]/], ["abc", /z/],
  ["a1b2", /(\d)(?=b)/]
];
for (var i = 0; i < cases.length; i++) {
  print(JSON.stringify(cases[i][0].split(cases[i][1])));
}
print(JSON.stringify("a,b,c".split(",", 2)));
print(JSON.stringify("a,b,c".split(/,/, 0)));
print("abc".search(/b/), "abc".search(/z/), "abc".search("c"));
print(JSON.stringify("abc".match(/z/)), JSON.stringify("abc".match(/z/g)));
print(JSON.stringify("a1a2".match(/a(\d)/g)));

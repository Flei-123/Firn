// Strings: the methods and the UTF-16 view of the world.
var s = "Hello, World";
print(s.length, s.charAt(4), s.charCodeAt(0), s[1]);
print(s.indexOf("o"), s.lastIndexOf("o"), s.slice(-5), s.substring(7, 12));
print(s.toUpperCase(), s.toLowerCase(), "  pad  ".trim() + "|");
print(s.split(", ").join("|"), "a".repeat(3), "x".padStart(4, "-"));
print("abc".startsWith("ab"), "abc".endsWith("bc"), "abc".includes("b"));
var e = "a\u{1F600}b";
print(e.length, e.charCodeAt(1), e.codePointAt(1), Array.from(e).length);
print(JSON.stringify("q\"\\\n\t"), String.fromCharCode(65, 66));
print("abc".replace("b", "X"), `t${1 + 1}v`);

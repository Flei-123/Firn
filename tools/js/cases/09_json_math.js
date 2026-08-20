// JSON and Math.
var v = { n: 1.5, s: "t\"x", b: [true, null, undefined], o: { d: 4 }, f: function(){} };
print(JSON.stringify(v));
print(JSON.stringify(v, null, 2));
print(JSON.stringify([1, undefined, function(){}]));
var back = JSON.parse('{"a":[1,2,{"b":"c"}],"d":null}');
print(back.a[2].b, back.d, JSON.stringify(back));
print(Math.abs(-3), Math.floor(-1.5), Math.ceil(-1.5), Math.round(-1.5), Math.round(2.5));
print(Math.max(1, 2, 3), Math.min(), Math.max(), Math.sign(-4), Math.trunc(-1.9));
print(Math.pow(2, 53), Math.sqrt(16), Math.hypot(3, 4), Math.cbrt(27));
print(Number.MAX_SAFE_INTEGER, Number.EPSILON, Number.isInteger(5.0), Number.isNaN(NaN));

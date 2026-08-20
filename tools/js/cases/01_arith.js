// Numbers, coercions and the printing of doubles.
print(1 + 1, 2 - 5, 3 * 4, 10 / 4, 10 % 3, 2 ** 10);
print(0.1 + 0.2, 1e21, 1e-7, 1 / 3, -0);
print(9007199254740993, 0.000001, 1234567890123456789);
print((5).toString(2), (255).toString(16), (0.5).toString(2));
print(1 / 0, -1 / 0, 0 / 0, Math.sqrt(-1));
print("5" * "2", "5" + 2, [] + {}, [1,2] + "");
print(+"", +" 12 ", +"0x10", +"abc", +null, +undefined, +true);
print(parseInt("0x1f"), parseInt("12px"), parseFloat("3.5e2xyz"));
print((123.456).toFixed(2), (0).toFixed(0), (1e21).toFixed(2));

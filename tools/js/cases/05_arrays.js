// Arrays and their methods.
var a = [5, 3, 9, 1];
print(a.length, a.join("-"), a.slice(1, 3).join(","));
print(a.concat([7]).join(","), a.indexOf(9), a.includes(4));
print(a.map(function (x) { return x * 2; }).join(","));
print(a.filter(function (x) { return x > 3; }).join(","));
print(a.reduce(function (s, x) { return s + x; }, 0));
print(a.slice().sort().join(","), a.slice().sort(function (x, y) { return y - x; }).join(","));
var b = [1, 2, 3, 4, 5];
print(b.splice(1, 2).join(","), b.join(","));
b.push(9); b.unshift(0);
print(b.join(","), b.pop(), b.shift(), b.join(","));
print([1, [2, [3]]].toString(), Array.isArray([]), Array.from("abc").join("|"));
var sparse = [1, , 3];
print(sparse.length, 1 in sparse, sparse.join(","));
print([..."hi", ...[1, 2]].join(","));

// Objects, prototypes, property attributes, getters and setters.
var o = { a: 1, get b() { return this.a * 2; }, set b(v) { this.a = v; } };
print(o.a, o.b);
o.b = 10;
print(o.a, o.b);
var p = Object.create(o);
print(p.a, Object.getPrototypeOf(p) === o, p.hasOwnProperty("a"));
Object.defineProperty(p, "c", { value: 7, enumerable: false });
print(p.c, Object.keys(p).length, JSON.stringify(Object.getOwnPropertyDescriptor(p, "c")));
var frozen = Object.freeze({ x: 1 });
frozen.x = 2;
print(frozen.x, Object.isFrozen(frozen), Object.isExtensible(frozen));
print(JSON.stringify(Object.assign({}, { a: 1 }, { b: 2 })));
function A(v) { this.v = v; }
A.prototype.twice = function () { return this.v * 2; };
var a = new A(21);
print(a.twice(), a instanceof A, Object.prototype.toString.call([]));

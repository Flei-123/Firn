// Classes, inheritance, static members, destructuring.
class Shape {
  constructor(name) { this.name = name; }
  describe() { return "a " + this.name; }
  static kind() { return "shape"; }
  get upper() { return this.name.toUpperCase(); }
}
class Square extends Shape {
  constructor(side) { super("square"); this.side = side; }
  area() { return this.side * this.side; }
  describe() { return super.describe() + " with area " + this.area(); }
}
var q = new Square(4);
print(q.describe(), q.upper, Square.kind(), q instanceof Shape);
var { name, side: s2 = 0 } = q;
print(name, s2);
var [x, , z = 9] = [1, 2];
print(x, z);
function g({ a = 1, b } = {}, ...rest) { return [a, b, rest.length].join(","); }
print(g(), g({ a: 5, b: 6 }, 7, 8));
print(JSON.stringify({ ...{ p: 1 }, q: 2 }));
var m = new Map([["k", 1]]); var st = new Set([1, 2, 2]);
print(m.get("k"), m.size, st.has(2), st.size);

// Closures, scope chains, `this`, hoisting.
function counter() { var n = 0; return function () { return ++n; }; }
var c = counter();
print(c(), c(), c());
var fns = [];
for (let i = 0; i < 3; i++) { fns.push(function () { return i; }); }
print(fns[0](), fns[1](), fns[2]());
var fns2 = [];
for (var j = 0; j < 3; j++) { fns2.push(function () { return j; }); }
print(fns2[0](), fns2[1](), fns2[2]());
print(typeof hoisted, hoisted());
function hoisted() { return "yes"; }
var obj = { v: 5, get: function () { return this.v; }, arrow: null };
obj.arrow = () => (typeof this);
print(obj.get(), obj.arrow());
print((function () { return typeof this; })());
print((function () { "use strict"; return typeof this; })());

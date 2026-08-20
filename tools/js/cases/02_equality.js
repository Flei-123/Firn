// == in its full ugliness, plus === and the relational operators.
print(null == undefined, null === undefined, null == 0, undefined == 0);
print(NaN == NaN, NaN === NaN, 0 == -0, "" == 0, "0" == 0, "" == "0");
print([] == false, [0] == false, [1] == true, ({}) == "[object Object]");
print("abc" < "abd", "Z" < "a", 2 < "10", "2" < "10");
print(1 <= 1, 2 >= 3, NaN < 1, NaN >= 1);
print(typeof null, typeof [], typeof function(){}, typeof Symbol());

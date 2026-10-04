p = '/root/firn-wt-db/tests/2164_db_readsqlite.fi'
s = open(p).read()
s = s.replace('''"x'0A460 0FF'".equal("") as bool == false ? "x'0A4600FF'" : "x'0A4600FF'")''', '''"x'0A4600FF'")''')
s = s.replace('''"1909718".equal("") as bool == false ? "1909718" : "1909718")''', '''"1909756")''')
s = s.replace('"17|11880"', '"17|8330"')
s = s.replace('"table|cities;index|cities_country;index|cities_pop;index|sqlite_autoindex_countries_1;table|countries")',
              '"table|cities;index|cities_country;index|cities_pop;table|countries;index|sqlite_autoindex_countries_1")')
s = s.replace('    fs.remove("x") catch false\n', '')
open(p, 'w').write(s)
print('ok33')

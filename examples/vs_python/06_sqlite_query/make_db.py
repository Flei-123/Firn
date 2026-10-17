# Generates shop.db deterministically (fixture for task.py and task.fi).
import os
import sqlite3

if os.path.exists("shop.db"):
    os.remove("shop.db")
con = sqlite3.connect("shop.db")
con.executescript("""
CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT, city TEXT);
CREATE TABLE orders(id INTEGER PRIMARY KEY, customer_id INTEGER, amount REAL);
""")
cities = ["Innsbruck", "Wien", "Graz"]
names = ["Anna", "Ben", "Cleo", "Dirk", "Eva", "Finn"]
con.executemany("INSERT INTO customers VALUES (?,?,?)", [(i + 1, n, cities[i % 3]) for i, n in enumerate(names)])
con.executemany("INSERT INTO orders VALUES (?,?,?)", [(i + 1, (i * 5) % 6 + 1, 10 + (i * 37) % 90 + 0.5) for i in range(25)])
con.commit()
con.close()

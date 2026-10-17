import sqlite3

con = sqlite3.connect("shop.db")
con.execute("INSERT INTO orders(customer_id, amount) VALUES (?, ?)", (2, 99.5))
rows = con.execute(
    """SELECT c.name, c.city, COUNT(*), SUM(o.amount)
       FROM customers c JOIN orders o ON o.customer_id = c.id
       GROUP BY c.id HAVING SUM(o.amount) > ?
       ORDER BY SUM(o.amount) DESC, c.name""",
    (100,),
)
for name, city, n, total in rows:
    print(f"{name:<6}{city:<10}{n:>3}{total:>9.1f}")
print("rowid", con.execute("SELECT last_insert_rowid()").fetchone()[0])
con.rollback()
con.close()

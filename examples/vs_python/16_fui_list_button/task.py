import tkinter as tk

root = tk.Tk()
root.title("Items")
root.geometry("320x300")
items = tk.Listbox(root)
items.pack(fill="both", expand=True)
name = tk.Entry(root)
name.pack(fill="x")


def add():
    if name.get():
        items.insert("end", name.get())
        name.delete(0, "end")


tk.Button(root, text="Add", command=add).pack()
root.mainloop()

from html.parser import HTMLParser

# html.parser has no CSS selectors: track the path of open tags by hand.
# (With bs4 this is: for a in soup.select("div.item > a[href]"): ...)
class Items(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []      # (tag, classes) of open elements
        self.found = []      # [href, text] of matching links
        self.title = ""
        self.cur = None
        self.in_title = False

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "title":
            self.in_title = True
        if tag == "a" and "href" in a and self.stack and self.stack[-1][0] == "div" \
                and "item" in self.stack[-1][1]:
            self.cur = [a["href"], ""]
            self.found.append(self.cur)
        if tag not in ("br", "img", "meta", "link", "input", "hr"):
            self.stack.append((tag, (a.get("class") or "").split()))

    def handle_endtag(self, tag):
        if tag == "title":
            self.in_title = False
        if tag == "a":
            self.cur = None
        for i in range(len(self.stack) - 1, -1, -1):
            if self.stack[i][0] == tag:
                del self.stack[i:]
                break

    def handle_data(self, data):
        if self.in_title:
            self.title += data
        if self.cur is not None:
            self.cur[1] += data

p = Items()
p.feed(open("page.html").read())
print("title:", p.title)
for href, text in p.found:
    print(f"{href:<6}{text}")

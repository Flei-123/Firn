#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""AT-SPI client for tools/fui/atspi_main.fi: walks the tree over D-Bus (dbus-python, no registry)
and compares every object with the a11y JSON oracle.   usage: atspi_test.py <bus-address> <json>
Prints one `OK`/`WRONG` line per check and the metric lines `METRIC name=value`."""
import json
import sys
import time

import dbus
import dbus.bus

ACC = "org.a11y.atspi.Accessible"
COMP = "org.a11y.atspi.Component"
ACT = "org.a11y.atspi.Action"
VAL = "org.a11y.atspi.Value"
APP = "org.a11y.atspi.Application"
PROPS = "org.freedesktop.DBus.Properties"

# AT-SPI role numbers, written down here independently of lib/fui/atspi.fi
# (checked against Atspi.Role of gir1.2-atspi-2.0 when run by hand).
ROLE = {
    "text": 29, "button": 43, "checkbox": 7, "radio": 44, "switch": 62, "slider": 51,
    "textbox": 79, "progressbar": 42, "combobox": 11, "list": 31, "listitem": 32,
    "tree": 65, "treeitem": 91, "table": 55, "row": 90, "cell": 56, "tab": 37,
    "tabpanel": 39, "menu": 33, "menuitem": 35, "toolbar": 63, "scrollbar": 48,
    "separator": 50, "group": 39, "heading": 83, "link": 88, "image": 27, "tooltip": 64,
    "status": 54, "menuitemcheckbox": 8, "menuitemradio": 45, "dialog": 16,
    "alertdialog": 2, "generic": 20, "columnheader": 25,
}
STATE = {"focusable": 11, "focused": 12, "selected": 23, "checked": 4, "mixed": 32,
         "pressed": 20, "expanded": 10, "collapsed": 5}
SENSITIVE, ENABLED, FRAME, APPLICATION = 24, 8, 23, 75

bad = 0
oks = 0


def CB_NOT_TEXT(c, path, acc):
    return "org.a11y.atspi.Text" not in [str(i) for i in c.call(path, acc, "GetInterfaces")]


def check(ok, what):
    global bad, oks
    if ok:
        oks += 1
        print("  ok    " + what)
    else:
        bad += 1
        print("  WRONG " + what)
    sys.stdout.flush()


class Client:
    def __init__(self, addr):
        self.bus = dbus.bus.BusConnection(addr)
        names = [str(n) for n in self.bus.list_names()]
        me = self.bus.get_unique_name()
        cand = [n for n in names if n.startswith(":") and n != me]
        if not cand:
            raise SystemExit("no application on the bus")
        self.app = cand[0]

    def obj(self, path):
        return self.bus.get_object(self.app, path, introspect=False)

    def call(self, path, iface, member, *args):
        return self.obj(path).get_dbus_method(member, iface)(*args)

    def prop(self, path, iface, name):
        return self.obj(path).Get(iface, name, dbus_interface=PROPS)


def bits(state_arr):
    s = set()
    for w, word in enumerate(state_arr):
        for b in range(32):
            if int(word) >> b & 1:
                s.add(w * 32 + b)
    return s


def main():
    addr, jpath = sys.argv[1], sys.argv[2]
    live = "--live" in sys.argv[3:]
    with open(jpath) as f:
        tree = json.load(f)
    c = Client(addr)
    root = "/org/a11y/atspi/accessible/root"

    # --- the application object
    check(c.call(root, ACC, "GetRole") == APPLICATION, "root: role APPLICATION (75)")
    check(str(c.prop(root, APP, "ToolkitName")) == "fUi", "root: ToolkitName fUi")
    check(str(c.prop(root, ACC, "Name")) == "Settings", "root: name is the window title")
    kids = c.call(root, ACC, "GetChildren")
    check(len(kids) == 1 and c.prop(root, ACC, "ChildCount") == 1, "root: exactly one child (the window)")
    frame_path = str(kids[0][1])

    stats = {"json": 0, "atspi": 0, "match": 0, "role": 0, "name": 0, "state": 0,
             "rect": 0, "value": 0, "parent": 0, "index": 0}
    paths = {}
    firsts = {}

    def walk(jn, path, parent_path, idx, is_frame):
        stats["json"] += 1
        stats["atspi"] += 1
        label = "%s %r" % (jn["role"], jn.get("name", ""))
        paths[label + "@" + path] = path
        role = c.call(path, ACC, "GetRole")
        want_role = FRAME if is_frame else ROLE[jn["role"]]
        r_ok = role == want_role
        name = str(c.prop(path, ACC, "Name"))
        want_name = jn.get("name", "")
        if is_frame and not want_name:
            want_name = "Settings"
        n_ok = name == want_name
        st = bits(c.call(path, ACC, "GetState"))
        s_ok = True
        for k, num in STATE.items():
            if live and k == "focused":
                continue
            if (k in jn.get("states", [])) != (num in st):
                s_ok = False
        if ("disabled" in jn.get("states", [])) == (SENSITIVE in st):
            s_ok = False
        ex = c.call(path, COMP, "GetExtents", dbus.UInt32(1))
        j = jn["rect"]
        rect_ok = live or [int(v) for v in ex] == j
        v_ok = True
        if "value" in jn:
            cur = float(c.prop(path, VAL, "CurrentValue"))
            v_ok = int(cur) == jn["value"] and float(c.prop(path, VAL, "MaximumValue")) == jn["max"]
            ifs = [str(i) for i in c.call(path, ACC, "GetInterfaces")]
            v_ok = v_ok and VAL in ifs
        pr = c.prop(path, ACC, "Parent")
        p_ok = str(pr[1]) == parent_path
        ix_ok = c.call(path, ACC, "GetIndexInParent") == idx
        stats["role"] += r_ok
        stats["name"] += n_ok
        stats["state"] += s_ok
        stats["rect"] += rect_ok
        stats["value"] += v_ok
        stats["parent"] += p_ok
        stats["index"] += ix_ok
        if r_ok and n_ok and s_ok and rect_ok and v_ok and p_ok and ix_ok:
            stats["match"] += 1
        else:
            print("    mismatch %s role=%s/%s name=%r/%r state=%s rect=%s/%s val=%s parent=%s idx=%s" % (
                path, role, want_role, name, want_name, s_ok, [int(v) for v in ex], j, v_ok, p_ok, ix_ok))
        ach = c.call(path, ACC, "GetChildren")
        jch = jn.get("children", [])
        if live and jn["role"] == "list" and jch and jch[0].get("virtual"):
            # the real window may show a different number of rows than the headless one
            jch = jch[:len(ach)]
        if len(ach) != len(jch) or c.prop(path, ACC, "ChildCount") != len(jch):
            print("    child count %s: atspi %d json %d" % (path, len(ach), len(jch)))
            stats["match"] -= 0
            return
        for k, (jc, ac) in enumerate(zip(jch, ach)):
            walk(jc, str(ac[1]), path, k, False)

    walk(tree, frame_path, root, 0, True)
    print("METRIC json_nodes=%d" % stats["json"])
    print("METRIC atspi_nodes=%d" % stats["atspi"])
    print("METRIC matching_nodes=%d" % stats["match"])
    check(stats["atspi"] == stats["json"] and stats["json"] > 20, "AT-SPI reaches every node of the JSON tree (%d of %d)" % (stats["atspi"], stats["json"]))
    check(stats["match"] == stats["json"], "role/name/state/rect/value/parent/index agree for all (%d of %d)" % (stats["match"], stats["json"]))

    # --- named lookups
    def find(role, name):
        for k, p in paths.items():
            if k.startswith("%s %r@" % (role, name)):
                return p
        return None

    cb = find("checkbox", "Dark mode")
    check(cb is not None, "check box 'Dark mode' is in the tree")
    ifs = [str(i) for i in c.call(cb, ACC, "GetInterfaces")]
    check(ACC in ifs and COMP in ifs and ACT in ifs, "check box offers Accessible, Component, Action")
    check(str(c.prop(cb, ACC, "Description")) == "Switches the dark theme", "description is served")
    h = find("heading", "Preferences")
    attrs = dict((str(k), str(v)) for k, v in c.call(h, ACC, "GetAttributes").items())
    check(attrs.get("level") == "1" and attrs.get("toolkit") == "fUi", "heading: attribute level=1, toolkit=fUi")
    check(int(c.call(cb, ACT, "GetNActions")) == 1 and str(c.call(cb, ACT, "GetName", 0)) == "click", "Action: one action 'click'")
    all_props = c.obj(cb).GetAll(ACC, dbus_interface=PROPS)
    check(str(all_props["Name"]) == "Dark mode" and int(all_props["ChildCount"]) == 0, "Properties.GetAll(Accessible)")

    # --- the virtual list rows are objects
    lst = find("list", "Big list")
    lk = c.call(lst, ACC, "GetChildren")
    check(len(lk) > 3 and all(str(k[1]).startswith("/org/a11y/atspi/accessible/v") for k in lk), "virtual list: visible rows are objects (%d)" % len(lk))
    check(str(c.prop(str(lk[0][1]), ACC, "Name")) == "Row 00", "virtual row 0 is named 'Row 00'")

    # --- the table: rows are TABLE_ROW objects, their cells TABLE_CELL objects (paths c<node>_<row>_<col>)
    tb = find("table", "People")
    check(tb is not None, "table 'People' is in the tree")
    tkall = c.call(tb, ACC, "GetChildren")
    tk = [k for k in tkall if str(k[1]).startswith("/org/a11y/atspi/accessible/v")]
    check(len(tk) == 2 and len(tkall) == 5, "table: the rows are objects (%d) and the 3 header cells follow (%d children)" % (len(tk), len(tkall)))
    check(c.call(str(tk[0][1]), ACC, "GetRole") == 90, "table row has the role TABLE_ROW (90)")
    ck = c.call(str(tk[1][1]), ACC, "GetChildren")
    check(len(ck) == 3 and all(str(k[1]).startswith("/org/a11y/atspi/accessible/c") for k in ck), "table row: 3 cell objects")
    check(c.call(str(ck[1][1]), ACC, "GetRole") == 56, "table cell has the role TABLE_CELL (56)")
    check([str(c.prop(str(k[1]), ACC, "Name")) for k in ck] == ["Bruno", "25", "b-02"], "table cells are named by their text")
    check(str(c.prop(str(ck[2][1]), ACC, "Parent")[1]) == str(tk[1][1]), "cell parent is its row")
    check(c.call(str(ck[2][1]), ACC, "GetIndexInParent") == 2, "cell index in parent is its column")

    # --- W42: the Table interface, TableCell and the header cells (columnheader)
    TBL, TCELL = "org.a11y.atspi.Table", "org.a11y.atspi.TableCell"
    NULLP = "/org/a11y/atspi/null"
    tifs = [str(i) for i in c.call(tb, ACC, "GetInterfaces")]
    check(TBL in tifs and TBL not in [str(i) for i in c.call(cb, ACC, "GetInterfaces")], "table offers the Table interface, a check box does not")
    check(TCELL in [str(i) for i in c.call(str(ck[0][1]), ACC, "GetInterfaces")], "table cell offers TableCell")
    check(int(c.prop(tb, TBL, "NRows")) == 2 and int(c.prop(tb, TBL, "NColumns")) == 3, "Table: NRows 2, NColumns 3")
    ga = c.prop  # alias: properties by name
    tall = c.obj(tb).GetAll(TBL, dbus_interface=PROPS)
    check(int(tall["NRows"]) == 2 and int(tall["NColumns"]) == 3 and int(tall["NSelectedRows"]) == 0 and int(tall["NSelectedColumns"]) == 0,
          "Properties.GetAll(Table)")
    check(str(c.prop(tb, TBL, "Caption")[1]) == NULLP and str(c.prop(tb, TBL, "Summary")[1]) == NULLP, "Table: Caption / Summary are the null reference")
    check(str(c.call(tb, TBL, "GetAccessibleAt", 1, 2)[1]) == str(ck[2][1]), "Table.GetAccessibleAt(1,2) is the cell of row 1, column 2")
    try:
        c.call(tb, TBL, "GetAccessibleAt", 9, 0)
        check(False, "GetAccessibleAt out of range gives an error")
    except dbus.DBusException as e:
        check("InvalidArgs" in str(e), "GetAccessibleAt out of range gives InvalidArgs")
    check(c.call(tb, TBL, "GetIndexAt", 1, 2) == 5 and c.call(tb, TBL, "GetIndexAt", 2, 0) == -1, "Table.GetIndexAt(1,2) = 5, out of range = -1")
    check(c.call(tb, TBL, "GetRowAtIndex", 5) == 1 and c.call(tb, TBL, "GetColumnAtIndex", 5) == 2 and c.call(tb, TBL, "GetRowAtIndex", 6) == -1,
          "Table.GetRowAtIndex(5) = 1, GetColumnAtIndex(5) = 2, index 6 = -1")
    ext = [int(v) for v in c.call(tb, TBL, "GetRowColumnExtentsAtIndex", 4)]
    check(ext[0] == 1 and ext[1:5] == [1, 1, 1, 1] and ext[5] == 0, "Table.GetRowColumnExtentsAtIndex(4) = row 1, col 1, 1x1, not selected")
    check(c.call(tb, TBL, "GetRowExtentAt", 0, 0) == 1 and c.call(tb, TBL, "GetColumnExtentAt", 1, 2) == 1 and c.call(tb, TBL, "GetColumnExtentAt", 5, 0) == 0,
          "Table: row/column extents are 1 (0 out of range)")
    check([str(c.call(tb, TBL, "GetColumnDescription", i)) for i in range(3)] == ["Name", "Age", "ID"] and str(c.call(tb, TBL, "GetRowDescription", 0)) == "",
          "Table.GetColumnDescription = the header titles")
    heads = [k for k in tkall if str(k[1]).startswith("/org/a11y/atspi/accessible/h")]
    check(len(heads) == 3 and [str(k[1]) for k in tkall[-3:]] == [str(k[1]) for k in heads], "header cells are the last 3 children of the table")
    check([c.call(str(k[1]), ACC, "GetRole") for k in heads] == [25, 25, 25], "header cells have the role COLUMN_HEADER (25)")
    check(str(c.call(str(heads[0][1]), ACC, "GetRoleName")) == "columnheader", "header role name is 'columnheader'")
    check([str(c.prop(str(k[1]), ACC, "Name")) for k in heads] == ["Name", "Age", "ID"], "header cells are named by their column titles")
    check(str(c.prop(str(heads[1][1]), ACC, "Parent")[1]) == tb and c.call(str(heads[1][1]), ACC, "GetIndexInParent") == 3, "header parent is the table, index = after the rows")
    check(str(c.call(tb, TBL, "GetColumnHeader", 1)[1]) == str(heads[1][1]) and str(c.call(tb, TBL, "GetColumnHeader", 7)[1]) == NULLP,
          "Table.GetColumnHeader(1) is the header cell (null out of range)")
    check(str(c.call(tb, TBL, "GetRowHeader", 0)[1]) == NULLP, "Table.GetRowHeader is the null reference (no row headers)")
    hx = [int(v) for v in c.call(str(heads[1][1]), COMP, "GetExtents", dbus.UInt32(0))]
    tx = [int(v) for v in c.call(tb, COMP, "GetExtents", dbus.UInt32(0))]
    cx = [int(v) for v in c.call(str(ck[1][1]), COMP, "GetExtents", dbus.UInt32(0))]
    check(hx[2] > 0 and hx[3] > 0 and hx[1] < cx[1] and hx[0] == cx[0] and hx[2] == cx[2] and hx[1] >= tx[1], "header cell: extent above its column's body cells, same x and width")
    hifs = [str(i) for i in c.call(str(heads[0][1]), ACC, "GetInterfaces")]
    check(COMP in hifs and ACT in hifs and TBL not in hifs, "header cell offers Component and Action")
    check(c.prop(str(ck[2][1]), TCELL, "Position") == (1, 2) or [int(v) for v in c.prop(str(ck[2][1]), TCELL, "Position")] == [1, 2], "TableCell.Position = (row 1, column 2)")
    check(int(c.prop(str(ck[2][1]), TCELL, "RowSpan")) == 1 and int(c.prop(str(ck[2][1]), TCELL, "ColumnSpan")) == 1, "TableCell: spans are 1")
    check(str(c.prop(str(ck[2][1]), TCELL, "Table")[1]) == tb, "TableCell.Table is the table")
    chc = c.call(str(ck[1][1]), TCELL, "GetColumnHeaderCells")
    check(len(chc) == 1 and str(chc[0][1]) == str(heads[1][1]) and len(c.call(str(ck[1][1]), TCELL, "GetRowHeaderCells")) == 0,
          "TableCell.GetColumnHeaderCells = the header of its column, no row headers")
    rcs = [int(v) for v in c.call(str(ck[1][1]), TCELL, "GetRowColumnSpan")]
    check(rcs == [1, 1, 1, 1, 1], "TableCell.GetRowColumnSpan = (row 1, col 1, 1x1)")
    check(c.call(tb, TBL, "IsRowSelected", 0) == 0 and len(c.call(tb, TBL, "GetSelectedRows")) == 0, "no row is selected at first")
    time.sleep(0.2)
    check(bool(c.call(str(tk[0][1]), ACT, "DoAction", 0)), "DoAction on a row answers true (a click in the row)")
    time.sleep(0.4)
    check(c.call(tb, TBL, "IsRowSelected", 0) == 1 and c.call(tb, TBL, "IsRowSelected", 1) == 0 and c.call(tb, TBL, "IsSelected", 0, 1) == 1,
          "after the click row 0 is selected (IsRowSelected / IsSelected)")
    check([int(v) for v in c.call(tb, TBL, "GetSelectedRows")] == [0] and int(c.prop(tb, TBL, "NSelectedRows")) == 1, "Table.GetSelectedRows = [0], NSelectedRows 1")
    check(23 in bits(c.call(str(tk[0][1]), ACC, "GetState")), "the selected row has the state SELECTED")
    check(c.call(tb, TBL, "GetSelectedColumns") == [] and c.call(tb, TBL, "IsColumnSelected", 0) == 0 and c.call(tb, TBL, "AddRowSelection", 1) == 0,
          "no column selection; AddRowSelection is refused (read only)")
    # a header Invoke sorts like the mouse: two clicks on 'Name' = descending, 'Bruno' comes first
    check(bool(c.call(str(heads[0][1]), ACT, "DoAction", 0)), "DoAction on the 'Name' header answers true")
    time.sleep(0.4)
    check(bool(c.call(str(heads[0][1]), ACT, "DoAction", 0)), "second DoAction on the 'Name' header answers true")
    time.sleep(0.4)
    tk2 = [k for k in c.call(tb, ACC, "GetChildren") if str(k[1]).startswith("/org/a11y/atspi/accessible/v")]
    first = [str(c.prop(str(k[1]), ACC, "Name")) for k in c.call(str(tk2[0][1]), ACC, "GetChildren")]
    check(first[0] == "Bruno", "the header click sorted the table: first row is now %r" % first[:1])

    # --- hit test over the bus
    ex = c.call(cb, COMP, "GetExtents", dbus.UInt32(0))
    hit = c.call(frame_path, COMP, "GetAccessibleAtPoint", int(ex[0]) + int(ex[2]) // 2, int(ex[1]) + int(ex[3]) // 2, dbus.UInt32(0))
    check(str(hit[1]) == cb, "GetAccessibleAtPoint at the check box centre returns the check box")

    # --- the action path: DoAction -> click -> state changes
    time.sleep(0.3)
    before = bits(c.call(cb, ACC, "GetState"))
    done = c.call(cb, ACT, "DoAction", 0)
    after = bits(c.call(cb, ACC, "GetState"))
    time.sleep(0.3)
    after = bits(c.call(cb, ACC, "GetState"))
    check(bool(done) and (4 in after) != (4 in before), "DoAction flips CHECKED over the bus (%s -> %s)" % (4 in before, 4 in after))
    check(c.call(cb, ACT, "DoAction", 1) == 0, "DoAction with a bad index answers false")
    try:
        c.call("/org/a11y/atspi/accessible/n9999", ACC, "GetRole")
        check(False, "unknown object gives an error")
    except dbus.DBusException as e:
        check("UnknownObject" in str(e), "unknown object gives UnknownObject")

    # --- events and the Text interface (r219): signals are caught on a second connection
    from dbus.mainloop.glib import DBusGMainLoop
    from gi.repository import GLib
    addr = sys.argv[1]
    ebus = dbus.bus.BusConnection(addr, mainloop=DBusGMainLoop(set_as_default=False))
    got = []

    def on_sig(*args, **kw):
        got.append((str(kw["path"]), str(kw["member"]), args))

    ebus.add_signal_receiver(on_sig, dbus_interface="org.a11y.atspi.Event.Object",
                             path_keyword="path", member_keyword="member")

    def pump(sec):
        t = time.time() + sec
        ctx = GLib.MainContext.default()
        while time.time() < t:
            while ctx.iteration(False):
                pass
            time.sleep(0.02)

    def seen(path, member, typ=None, d1=None):
        for pth, mem, a in got:
            if pth == path and mem == member and (typ is None or str(a[0]) == typ) and (d1 is None or int(a[1]) == d1):
                return True
        return False

    pump(0.5)
    got[:] = []
    c.call(cb, ACT, "DoAction", 0)
    pump(0.8)
    check(seen(cb, "StateChanged", "checked"), "event: StateChanged 'checked' after DoAction on the check box")
    check(any(mem == "Announcement" and int(a[1]) == 2 and str(a[3]) == "Action done" for pth, mem, a in got),
          "event: Announcement 'Action done' (assertive) after DoAction")
    tbs = [p for k, p in paths.items() if k.startswith("textbox ")]
    check(len(tbs) == 1, "one text field in the tree")
    tb = tbs[0]
    ifs = [str(i) for i in c.call(tb, ACC, "GetInterfaces")]
    TXT, ETX = "org.a11y.atspi.Text", "org.a11y.atspi.EditableText"
    check(TXT in ifs and ETX in ifs, "text field offers Text and EditableText")
    check(CB_NOT_TEXT(c, cb, ACC), "check box does not offer Text")
    check(int(c.prop(tb, TXT, "CharacterCount")) == 0, "Text: CharacterCount 0 for the empty field")
    c.call(tb, COMP, "GrabFocus")
    pump(0.6)
    check(seen(tb, "StateChanged", "focused", 1), "event: StateChanged 'focused' after GrabFocus")
    got[:] = []
    text = u"h\u00e9llo w\u00f6rld"
    check(bool(c.call(tb, ETX, "InsertText", 0, text, -1)), "EditableText.InsertText")
    pump(0.8)
    check(str(c.call(tb, TXT, "GetText", 0, -1)) == text, "Text.GetText(0,-1) returns the text (umlauts)")
    check(int(c.prop(tb, TXT, "CharacterCount")) == 11, "Text: CharacterCount counts codepoints (11, not 13 bytes)")
    check(seen(tb, "TextChanged", "insert", 0), "event: TextChanged insert at 0")
    check(str(c.call(tb, TXT, "GetText", 1, 4)) == u"\u00e9ll", "Text.GetText(1,4) = 'ell' with accent")
    check(int(c.call(tb, TXT, "GetCharacterAtOffset", 1)) == 0xe9, "Text.GetCharacterAtOffset(1) = e-acute")
    w = c.call(tb, TXT, "GetStringAtOffset", 8, dbus.UInt32(1))
    check(str(w[0]) == u"w\u00f6rld" and int(w[1]) == 6 and int(w[2]) == 11, "Text.GetStringAtOffset word at 8 = 'woerld' (6..11)")
    got[:] = []
    check(bool(c.call(tb, TXT, "SetCaretOffset", 5)), "Text.SetCaretOffset(5)")
    pump(0.8)
    check(int(c.call(tb, TXT, "GetCaretOffset")) == 5 and int(c.prop(tb, TXT, "CaretOffset")) == 5, "Text: caret offset 5")
    check(seen(tb, "TextCaretMoved", None, 5), "event: TextCaretMoved to 5")
    got[:] = []
    check(bool(c.call(tb, TXT, "SetSelection", 0, 1, 4)), "Text.SetSelection(0,1,4)")
    sel = c.call(tb, TXT, "GetSelection", 0)
    check(int(c.call(tb, TXT, "GetNSelections")) == 1 and (int(sel[0]), int(sel[1])) == (1, 4), "Text: one selection (1,4)")
    pump(0.6)
    check(seen(tb, "TextSelectionChanged"), "event: TextSelectionChanged")
    got[:] = []
    check(bool(c.call(tb, ETX, "DeleteText", 0, 6)), "EditableText.DeleteText(0,6)")
    pump(0.8)
    check(str(c.call(tb, TXT, "GetText", 0, -1)) == u"w\u00f6rld", "text after DeleteText is 'woerld'")
    dels = [a for pth, mem, a in got if pth == tb and mem == "TextChanged" and str(a[0]) == "delete"]
    check(len(dels) == 1 and int(dels[0][1]) == 0 and int(dels[0][2]) == 6 and str(dels[0][3]) == u"h\u00e9llo ",
          "event: TextChanged delete at 0, length 6, text 'h\u00e9llo '")
    got[:] = []
    check(bool(c.call(tb, ETX, "SetTextContents", u"xy")), "EditableText.SetTextContents")
    pump(0.8)
    check(seen(tb, "TextChanged", "insert"), "event: TextChanged after SetTextContents")
    # name / value events: the slider is moved by DoAction? not operable by click centre -> only check no crash
    check(len(got) > 0 and all(len(a) == 5 for _, _, a in got), "all signals have the 5-field body siiva{sv}")

    print("METRIC checks_ok=%d" % oks)
    print("METRIC checks_wrong=%d" % bad)
    # stop the application
    q = find("button", "Quit")
    c.call(q, ACT, "DoAction", 0)
    time.sleep(0.2)
    print("ATSPI TEST " + ("PASSED" if bad == 0 else "FAILED"))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())

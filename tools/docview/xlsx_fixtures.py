import datetime
import os
import sys

import openpyxl
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side


def basic(path):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Data"
    for c in "ABCD":
        ws[c + "1"].font = Font(bold=True)
        ws[c + "1"].fill = PatternFill("solid", fgColor="FFFF00")
    ws["A1"], ws["B1"], ws["C1"], ws["D1"] = "Name", "Amount", "Date", "Pct"
    ws["A2"], ws["B2"], ws["C2"], ws["D2"] = "Apple", 1234.5, datetime.date(2024, 3, 15), 0.256
    ws["A3"], ws["B3"], ws["C3"], ws["D3"] = "Banana", -7.125, datetime.date(2020, 12, 1), 1
    ws["B2"].number_format = "#,##0.00"
    ws["C2"].number_format = "dd.mm.yyyy"
    ws["C3"].number_format = "mmm d, yyyy"
    ws["D2"].number_format = "0.0%"
    ws["D3"].number_format = "0%"
    ws["B3"].number_format = "0.00"
    ws.merge_cells("A5:C6")
    ws["A5"] = "Merged"
    ws["A5"].alignment = Alignment(horizontal="center")
    ws["A5"].border = Border(left=Side(style="medium"), bottom=Side(style="thin"))
    ws.column_dimensions["A"].width = 20
    ws["A8"] = 3.14159265358979
    ws["B8"] = "line1\nline2"
    ws["B8"].alignment = Alignment(wrap_text=True)
    ws2 = wb.create_sheet("Second")
    ws2["B2"] = "hello"
    ws2["C3"] = True
    wb.save(path)


def wide(path):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Wide"
    for r in range(1, 60):
        for c in range(1, 30):
            ws.cell(r, c, r * c if (r + c) % 3 else "t%d_%d" % (r, c))
    ws.column_dimensions["C"].hidden = True
    ws.row_dimensions[5].hidden = True
    ws.row_dimensions[7].height = 40
    ws.sheet_view.showGridLines = False
    wb.save(path)


def formats(path):
    wb = openpyxl.Workbook()
    ws = wb.active
    codes = ["General", "0", "0.00", "#,##0", "0.00E+00", "0%", "$#,##0.00", "yyyy-mm-dd hh:mm:ss", "h:mm AM/PM", "[h]:mm", "@", '0.0 "kg"', "#,##0;[Red](#,##0)"]
    for i, code in enumerate(codes, start=1):
        ws.cell(i, 1, code)
        ws.cell(i, 2, 45123.6875 if "h" in code or "y" in code else 1234567.891).number_format = code
        ws.cell(i, 3, -0.5).number_format = code
    wb.save(path)


def main(out):
    os.makedirs(out, exist_ok=True)
    basic(os.path.join(out, "basic.xlsx"))
    wide(os.path.join(out, "wide.xlsx"))
    formats(os.path.join(out, "formats.xlsx"))


if __name__ == "__main__":
    main(sys.argv[1])

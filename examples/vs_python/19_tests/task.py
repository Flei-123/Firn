import sys
import unittest

import money


class MoneyTest(unittest.TestCase):
    def setUp(self):  # fixture: a price list
        self.prices = {"apple": "0.40", "bread": "2.5", "cheese": "12"}

    def assertCents(self, text, cents):  # assert helper
        self.assertEqual(money.parse_cents(text), cents, text)

    def test_parse_valid(self):
        for text, cents in [("12.34", 1234), ("5", 500), ("5.5", 550), ("0.05", 5)]:
            with self.subTest(text=text):
                self.assertCents(text, cents)

    def test_parse_invalid(self):
        for text in ["", "abc", "1.234", "1..2", ".5", "-3"]:
            with self.subTest(text=text):
                self.assertIsNone(money.parse_cents(text))

    def test_format(self):
        for cents, text in [(0, "0.00"), (5, "0.05"), (1234, "12.34"), (100000, "1000.00")]:
            with self.subTest(cents=cents):
                self.assertEqual(money.format_cents(cents), text)

    def test_roundtrip(self):
        for name, text in self.prices.items():
            with self.subTest(name=name):
                self.assertEqual(money.parse_cents(money.format_cents(money.parse_cents(text))), money.parse_cents(text))

    def test_vat(self):
        for cents, pct, want in [(1000, 20, 1200), (999, 20, 1199), (1, 20, 1), (0, 20, 0)]:
            with self.subTest(cents=cents):
                self.assertEqual(money.add_vat(cents, pct), want)

    def test_vat_zero_percent_is_identity(self):
        for text in self.prices.values():
            self.assertEqual(money.add_vat(money.parse_cents(text), 0), money.parse_cents(text))

    def test_share_sums_to_total(self):
        for total, n in [(100, 3), (1, 5), (999, 7), (0, 4)]:
            with self.subTest(total=total, n=n):
                self.assertEqual(sum(money.share(total, n, i) for i in range(n)), total)

    def test_share_differs_by_at_most_one_cent(self):
        parts = [money.share(1000, 7, i) for i in range(7)]
        self.assertLessEqual(max(parts) - min(parts), 1)


result = unittest.TextTestRunner(stream=open("/dev/null", "w")).run(unittest.defaultTestLoader.loadTestsFromTestCase(MoneyTest))
print(f"{result.testsRun} tests, {len(result.failures) + len(result.errors)} failed")
sys.exit(not result.wasSuccessful())

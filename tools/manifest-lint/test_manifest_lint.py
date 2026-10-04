#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Tests of manifest_lint.py: the real manifests pass, every rule has a manifest that breaks exactly it."""
import os
import unittest

import manifest_lint as ml

HERE = os.path.dirname(os.path.abspath(__file__))
GOOD = '''manifest 1
app demo
title "Demo"
# api 1.2.3
action demo.add write "Add a thing"
  arg text string required "The text"
  arg at int optional "Where"
  returns count int "How many now"
  dryrun
  undo demo.remove
action demo.remove write "Remove a thing"
  arg index int required "Which"
  returns count int "How many now"
action demo.clear critical "Delete everything"
  returns deleted int "How many"
  dryrun
action demo.list read "List"
  returns rows string "One per line"
event demo.changed "After every change"
  field count int "How many now"
'''


def errors(text, **kw):
    return ml.lint(text, 'x', **kw).errors


def warnings(text):
    return ml.lint(text, 'x').warnings


class Good(unittest.TestCase):
    def test_base_is_clean(self):
        r = ml.lint(GOOD, 'x')
        self.assertEqual(r.errors, [])
        self.assertEqual(r.warnings, [])
        self.assertEqual((r.app, r.api, len(r.actions), len(r.events)), ('demo', '1.2.3', 4, 1))

    def test_real_manifests(self):
        d = os.path.join(HERE, 'fixtures', 'good')
        names = sorted(os.listdir(d))
        self.assertGreaterEqual(len(names), 5)
        for n in names:
            r = ml.lint(open(os.path.join(d, n), encoding='utf8').read(), n)
            self.assertEqual(r.errors, [], n)

    def test_self_undo_is_allowed(self):
        t = GOOD.replace('  undo demo.remove', '  undo demo.add')
        self.assertEqual(errors(t), [])

    def test_wrapper_keywords_only_with_flag(self):
        t = GOOD.replace('action demo.list read "List"', 'action demo.list read "List"\n  adapter cli')
        self.assertTrue(any('unknown keyword' in e for e in errors(t)))
        self.assertEqual(errors(t, wrapper=True), [])


BREAK = [
    ('no header', lambda t: t.replace('manifest 1\n', '', 1), "first line must be 'manifest 1'"),
    ('wrong version', lambda t: t.replace('manifest 1', 'manifest 2', 1), "first line must be 'manifest 1'"),
    ('no app', lambda t: t.replace('app demo\n', '', 1), "before 'app'"),
    ('two apps', lambda t: t.replace('app demo\n', 'app demo\napp other\n', 1), "second 'app'"),
    ('bad app name', lambda t: t.replace('app demo', 'app Demo', 1), "'app' needs"),
    ('long app name', lambda t: t.replace('app demo', 'app abcdefghijklmn', 1), "'app' needs"),
    ('foreign namespace', lambda t: t.replace('action demo.list', 'action evil.list', 1), 'outside the namespace'),
    ('no name after dot', lambda t: t.replace('action demo.list', 'action demo.', 1), 'no name after the dot'),
    ('duplicate action', lambda t: t.replace('action demo.list read', 'action demo.add read', 1), 'declared twice'),
    ('bad level', lambda t: t.replace('demo.list read', 'demo.list admin', 1), 'level of'),
    ('missing doc quotes', lambda t: t.replace('action demo.list read "List"', 'action demo.list read List', 1), 'action line must be'),
    ('empty doc', lambda t: t.replace('"List"', '" "', 1), 'empty description'),
    ('bad type', lambda t: t.replace('arg at int', 'arg at float', 1), 'type must be'),
    ('arg without required', lambda t: t.replace('optional "Where"', 'maybe "Where"', 1), 'required or optional'),
    ('arg without doc', lambda t: t.replace('arg at int optional "Where"', 'arg at int optional', 1), 'arg line must be'),
    ('duplicate arg', lambda t: t.replace('  arg at int optional "Where"', '  arg text int optional "Where"', 1), "declared twice"),
    ('long arg name', lambda t: t.replace('arg at int', 'arg ' + 'a' * 24 + ' int', 1), 'longer than 23'),
    ('arg outside action', lambda t: 'manifest 1\napp demo\narg x int required "x"\n', "'arg' outside an action"),
    ('returns in event', lambda t: t.replace('  field count int "How many now"', '  returns count int "x"', 1), "'returns' outside an action"),
    ('field in action', lambda t: t.replace('  returns rows string "One per line"', '  field rows string "x"', 1), "'field' outside an event"),
    ('dryrun on read', lambda t: t.replace('  returns rows string "One per line"', '  returns rows string "x"\n  dryrun', 1), 'read action'),
    ('dryrun twice', lambda t: t.replace('  dryrun\n  undo', '  dryrun\n  dryrun\n  undo', 1), "'dryrun' twice"),
    ('undo missing action', lambda t: t.replace('undo demo.remove', 'undo demo.nothing', 1), 'not declared'),
    ('undo foreign app', lambda t: t.replace('undo demo.remove', 'undo evil.remove', 1), 'not an action of app'),
    ('undo on read', lambda t: t.replace('  returns rows string "One per line"', '  returns rows string "x"\n  undo demo.add', 1), 'read action'),
    ('unknown keyword', lambda t: t.replace('title "Demo"', 'title "Demo"\nversion 3', 1), 'unknown keyword'),
    ('tab', lambda t: t.replace('  arg at int', '\targ at int', 1), 'tab character'),
    ('title unquoted', lambda t: t.replace('title "Demo"', 'title Demo', 1), "'title' needs"),
    ('duplicate event', lambda t: t.replace('event demo.changed "After every change"', 'event demo.changed "a"\n  field n int "n"\nevent demo.changed "b"', 1), 'declared twice'),
]


def make_break_test(label, mutate, want):
    def t(self):
        text = mutate(GOOD)
        self.assertNotEqual(text, GOOD, 'the mutation changed nothing: ' + label)
        errs = errors(text)
        self.assertTrue(any(want in e for e in errs), '%s: wanted %r in %r' % (label, want, errs))
    return t


for i, (label, mutate, want) in enumerate(BREAK):
    setattr(Good, 'test_breaks_%02d_%s' % (i, label.replace(' ', '_')), make_break_test(label, mutate, want))


class Warn(unittest.TestCase):
    def test_no_api_comment(self):
        self.assertTrue(any('api X.Y' in w for w in warnings(GOOD.replace('# api 1.2.3\n', ''))))

    def test_critical_without_dryrun(self):
        t = GOOD.replace('  returns deleted int "How many"\n  dryrun', '  returns deleted int "How many"')
        self.assertTrue(any('no dryrun' in w for w in warnings(t)))

    def test_no_returns(self):
        t = GOOD.replace('  returns rows string "One per line"\n', '')
        self.assertTrue(any('no returns' in w for w in warnings(t)))

    def test_long_name_warns_only(self):
        t = GOOD.replace('demo.list', 'demo.' + 'l' * 30)
        self.assertEqual(errors(t), [])
        self.assertTrue(any('31 characters' in w for w in warnings(t)))

    def test_strict_turns_warnings_into_errors(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, 'ACTIONS')
            open(p, 'w').write(GOOD.replace('# api 1.2.3\n', ''))
            self.assertEqual(ml.main([p]), 0)
            self.assertEqual(ml.main(['--strict', p]), 1)


if __name__ == '__main__':
    unittest.main()

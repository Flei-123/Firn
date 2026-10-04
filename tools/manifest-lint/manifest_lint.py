#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""manifest-lint -- checks ACTIONS manifests (`manifest 1`) for every app of the Fleitec family.

One format, many readers: OpenPlan (`tools/model/opmodel.py`), LogicLab (`scripts/api/runtime.ts`), the OrientOS
action bus (`kernel/user/orientbus.fi`, host reader `tools/actionbus/manifest.py`) and now FirnChat, FreeViewer,
Daidalos. A manifest that passes here is read by ALL of them (the grammar below is the intersection, written
after the three parsers).

    manifest 1
    app <name>                                  [a-z][a-z0-9_]{0,12}, once, before every declaration
    title "..."                                 optional
    # api 1.0                                   SemVer of the API as a comment (warning when missing)
    action <app>.<verb> <read|write|critical> "doc"
      arg <name> <string|int|bool> <required|optional> "doc"
      returns <name> <string|int|bool> "doc"
      dryrun                                    write/critical only
      undo <app>.<action>                       write/critical only, an action of this manifest
    event <app>.<name> "doc"
      field <name> <string|int|bool> "doc"

Errors (exit 1): wrong/missing header, a name outside the app's namespace, duplicate names, unknown level/type,
missing or empty doc, malformed line, `undo` to a missing or foreign action (an action may undo itself), `dryrun`/`undo` on a read action,
`arg`/`returns` outside an action or in an event, `field` outside an event, unknown keywords.
Warnings: a name over 31 characters (the OrientOS bus refuses it), no `# api X.Y[.Z]` comment, a critical action without `dryrun`, an action without any `returns`.

    manifest_lint.py [--json] [--strict] [--wrapper] FILE...
      --json      print the catalogue of every file as JSON (name, level, doc, args, returns, dryrun, undo, events)
      --strict    warnings are errors
      --wrapper   allow the OrientOS wrapper keywords (`for`, `adapter`, `keyed`)
"""
import json
import re
import sys

LEVELS = ('read', 'write', 'critical')
TYPES = ('string', 'int', 'bool')
APP = re.compile(r'^[a-z][a-z0-9_]{0,12}$')
NAME = re.compile(r'^[a-z][a-z0-9_]*(\.[A-Za-z0-9_]+)+$')
ITEM = re.compile(r'^[A-Za-z][A-Za-z0-9_.]*$')
# the strict forms of the three parsers (the doc runs to the end of the line)
RE_ACTION = re.compile(r'^action (\S+) (\S+) "(.*)"$')
RE_EVENT = re.compile(r'^event (\S+) "(.*)"$')
RE_ARG = re.compile(r'^arg (\S+) (\S+) (\S+) "(.*)"$')
RE_RET = re.compile(r'^returns (\S+) (\S+) "(.*)"$')
RE_FIELD = re.compile(r'^field (\S+) (\S+) "(.*)"$')
RE_SEMVER_COMMENT = re.compile(r'^#\s*api\s+(\d+\.\d+(\.\d+)?)\b')
NAME_MAX = 31
ITEM_MAX = 23


class Result:
    def __init__(self, path):
        self.path = path
        self.errors = []
        self.warnings = []
        self.app = None
        self.title = ''
        self.api = None
        self.actions = []
        self.events = []

    def err(self, no, msg):
        self.errors.append('%s:%d: %s' % (self.path, no, msg))

    def warn(self, no, msg):
        self.warnings.append('%s:%d: %s' % (self.path, no, msg))


def lint(text, path='<manifest>', wrapper=False):
    r = Result(path)
    head = False
    cur = None          # the action or event being filled
    for no, raw in enumerate(text.split('\n'), 1):
        line = raw.strip()
        if not line:
            continue
        if line.startswith('#'):
            m = RE_SEMVER_COMMENT.match(line)
            if m and r.api is None:
                r.api = m.group(1)
            continue
        if '\t' in raw:
            r.err(no, 'tab character (use spaces)')
        w = line.split()
        k = w[0]
        if not head:
            if w != ['manifest', '1']:
                r.err(no, "first line must be 'manifest 1'")
                return r
            head = True
            continue
        if k == 'manifest':
            r.err(no, "a second 'manifest' line")
        elif k == 'app':
            if r.app is not None:
                r.err(no, "a second 'app' line")
            elif len(w) != 2 or not APP.match(w[1]):
                r.err(no, "'app' needs one name of 1..13 characters [a-z0-9_] starting with a letter")
            else:
                r.app = w[1]
        elif r.app is None:
            r.err(no, "'%s' before 'app'" % k)
        elif k == 'title':
            m = re.match(r'^title "(.*)"$', line)
            if not m or not m.group(1).strip():
                r.err(no, "'title' needs a quoted, non-empty text")
            else:
                r.title = m.group(1)
        elif k == 'action':
            m = RE_ACTION.match(line)
            if not m:
                r.err(no, 'action line must be: action <app>.<verb> <read|write|critical> "doc"')
                cur = None
                continue
            name, level, doc = m.groups()
            cur = {'name': name, 'level': level, 'doc': doc, 'args': [], 'returns': [], 'dryrun': False,
                   'undo': None, 'line': no, 'kind': 'action'}
            if not check_name(r, no, name, 'action'):
                cur['bad'] = True
            if level not in LEVELS:
                r.err(no, "level of '%s' must be read, write or critical (got '%s')" % (name, level))
            if not doc.strip():
                r.err(no, "action '%s' has an empty description" % name)
            if any(a['name'] == name for a in r.actions):
                r.err(no, "action '%s' declared twice" % name)
            r.actions.append(cur)
        elif k == 'event':
            m = RE_EVENT.match(line)
            if not m:
                r.err(no, 'event line must be: event <app>.<name> "doc"')
                cur = None
                continue
            name, doc = m.groups()
            cur = {'name': name, 'doc': doc, 'fields': [], 'line': no, 'kind': 'event'}
            check_name(r, no, name, 'event')
            if not doc.strip():
                r.err(no, "event '%s' has an empty description" % name)
            if any(e['name'] == name for e in r.events):
                r.err(no, "event '%s' declared twice" % name)
            r.events.append(cur)
        elif k == 'arg':
            if cur is None or cur['kind'] != 'action':
                r.err(no, "'arg' outside an action")
                continue
            m = RE_ARG.match(line)
            if not m:
                r.err(no, 'arg line must be: arg <name> <type> <required|optional> "doc"')
                continue
            name, typ, req, doc = m.groups()
            item_checks(r, no, 'arg', name, typ, doc, cur['args'])
            if req not in ('required', 'optional'):
                r.err(no, "arg '%s': must say required or optional (got '%s')" % (name, req))
            cur['args'].append({'name': name, 'type': typ, 'required': req == 'required', 'doc': doc})
        elif k == 'returns':
            if cur is None or cur['kind'] != 'action':
                r.err(no, "'returns' outside an action")
                continue
            m = RE_RET.match(line)
            if not m:
                r.err(no, 'returns line must be: returns <name> <type> "doc"')
                continue
            name, typ, doc = m.groups()
            item_checks(r, no, 'returns', name, typ, doc, cur['returns'])
            cur['returns'].append({'name': name, 'type': typ, 'doc': doc})
        elif k == 'field':
            if cur is None or cur['kind'] != 'event':
                r.err(no, "'field' outside an event")
                continue
            m = RE_FIELD.match(line)
            if not m:
                r.err(no, 'field line must be: field <name> <type> "doc"')
                continue
            name, typ, doc = m.groups()
            item_checks(r, no, 'field', name, typ, doc, cur['fields'])
            cur['fields'].append({'name': name, 'type': typ, 'doc': doc})
        elif k == 'dryrun':
            if cur is None or cur['kind'] != 'action':
                r.err(no, "'dryrun' outside an action")
            elif cur['level'] == 'read':
                r.err(no, "'dryrun' on a read action ('%s'): a read changes nothing" % cur['name'])
            elif cur['dryrun']:
                r.err(no, "'dryrun' twice in '%s'" % cur['name'])
            else:
                cur['dryrun'] = True
        elif k == 'undo':
            if cur is None or cur['kind'] != 'action':
                r.err(no, "'undo' outside an action")
            elif len(w) != 2:
                r.err(no, "'undo' needs exactly one action name")
            elif cur['level'] == 'read':
                r.err(no, "'undo' on a read action ('%s'): nothing to undo" % cur['name'])
            elif cur['undo']:
                r.err(no, "'undo' twice in '%s'" % cur['name'])
            else:
                cur['undo'] = w[1]
                cur['undo_line'] = no
        elif k in ('keyed', 'for', 'adapter') and wrapper:
            pass
        else:
            r.err(no, "unknown keyword '%s'" % k)
    if not head:
        r.err(1, "empty file (no 'manifest 1')")
        return r
    if r.app is None:
        r.err(1, "no 'app' line")
    names = {a['name'] for a in r.actions}
    for a in r.actions:
        if a['undo']:
            u = a['undo']
            # undo of itself is fine: a `set` is reversed by the same `set` with the earlier value
            if not u.startswith((r.app or '') + '.'):
                r.err(a['undo_line'], "undo '%s' is not an action of app '%s'" % (u, r.app))
            elif u not in names:
                r.err(a['undo_line'], "undo '%s' is not declared in this manifest" % u)
        if a['level'] == 'critical' and not a['dryrun']:
            r.warn(a['line'], "critical action '%s' has no dryrun" % a['name'])
        if not a['returns']:
            r.warn(a['line'], "action '%s' has no returns line" % a['name'])
    if r.api is None:
        r.warn(1, "no '# api X.Y[.Z]' comment (the SemVer of the API)")
    return r


def check_name(r, no, name, what):
    ok = True
    if r.app and not name.startswith(r.app + '.'):
        r.err(no, "%s '%s' is outside the namespace of app '%s'" % (what, name, r.app))
        ok = False
    elif r.app and len(name) == len(r.app) + 1:
        r.err(no, "%s '%s' has no name after the dot" % (what, name))
        ok = False
    if len(name) > NAME_MAX:
        # the OrientOS bus refuses longer names; the other readers do not care
        r.warn(no, "%s '%s' is longer than %d characters (the OrientOS bus refuses it)" % (what, name, NAME_MAX))
    if not NAME.match(name):
        r.err(no, "%s name '%s' does not match <app>.<verb> ([a-z0-9_], dots)" % (what, name))
        ok = False
    return ok


def item_checks(r, no, what, name, typ, doc, siblings):
    if not ITEM.match(name):
        r.err(no, "%s name '%s' is not an identifier" % (what, name))
    if len(name) > ITEM_MAX:
        r.err(no, "%s name '%s' is longer than %d characters" % (what, name, ITEM_MAX))
    if typ not in TYPES:
        r.err(no, "%s '%s': type must be string, int or bool (got '%s')" % (what, name, typ))
    if not doc.strip():
        r.err(no, "%s '%s' has an empty description" % (what, name))
    if any(s['name'] == name for s in siblings):
        r.err(no, "%s '%s' declared twice" % (what, name))


def catalogue(r):
    return {'app': r.app, 'title': r.title, 'api': r.api,
            'actions': [{'name': a['name'], 'level': a['level'], 'doc': a['doc'], 'args': a['args'],
                         'returns': a['returns'], 'dryrun': a['dryrun'], 'undo': a['undo']} for a in r.actions],
            'events': [{'name': e['name'], 'doc': e['doc'], 'fields': e['fields']} for e in r.events]}


def main(argv):
    as_json = '--json' in argv
    strict = '--strict' in argv
    wrapper = '--wrapper' in argv
    files = [a for a in argv if not a.startswith('--')]
    if not files:
        print(__doc__)
        return 2
    bad = False
    cats = {}
    for f in files:
        try:
            text = open(f, encoding='utf8').read()
        except OSError as e:
            print('FAIL %s: %s' % (f, e))
            bad = True
            continue
        r = lint(text, f, wrapper)
        for w in r.warnings:
            print('warning: ' + w, file=sys.stderr if as_json else sys.stdout)
        if r.errors or (strict and r.warnings):
            for e in r.errors:
                print('error: ' + e, file=sys.stderr if as_json else sys.stdout)
            bad = True
        elif not as_json:
            nargs = sum(len(a['args']) for a in r.actions)
            print('OK %s app=%s api=%s actions=%d events=%d args=%d' % (
                f, r.app, r.api or '-', len(r.actions), len(r.events), nargs))
        cats[f] = catalogue(r)
    if as_json:
        print(json.dumps(cats if len(files) > 1 else cats[files[0]], indent=1))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))

REGISTRY = []
PROJECT_REGISTRY = []


def register(rule_id, summary):
    def wrap(fn):
        REGISTRY.append((rule_id, summary, fn))
        return fn

    return wrap


def register_project(rule_id, summary):
    def wrap(fn):
        PROJECT_REGISTRY.append((rule_id, summary, fn))
        return fn

    return wrap


def finding_at(path, rule_id, line, text, message):
    return {"file": path, "line": line, "rule": rule_id, "text": text, "message": message}


def finding(ctx, rule_id, line, text, message):
    return {"file": ctx.path, "line": line, "rule": rule_id, "text": text, "message": message}


from . import nav, rawvalue, duplicate, apply_button, paging, placeholder, icon_install

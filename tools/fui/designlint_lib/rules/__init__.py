REGISTRY = []


def register(rule_id, summary):
    def wrap(fn):
        REGISTRY.append((rule_id, summary, fn))
        return fn

    return wrap


def finding(ctx, rule_id, line, text, message):
    return {"file": ctx.path, "line": line, "rule": rule_id, "text": text, "message": message}


from . import nav, rawvalue, duplicate, apply_button, paging, placeholder

import logging, logging.handlers, os, sys, tempfile

with tempfile.TemporaryDirectory() as d:
    fmt = logging.Formatter("%(asctime)s %(levelname)-8s %(name)s: %(message)s")
    fh = logging.handlers.RotatingFileHandler(os.path.join(d, "app.log"), maxBytes=250, backupCount=2)
    sh = logging.StreamHandler(sys.stderr)
    sh.setLevel(logging.WARNING)
    root = logging.getLogger()
    root.setLevel(logging.INFO)
    for h in (fh, sh):
        h.setFormatter(fmt)
        root.addHandler(h)
    app = logging.getLogger("app")
    db = logging.getLogger("app.db")
    db.setLevel(logging.DEBUG)
    app.debug("hidden")
    app.info("start %d", 1)
    db.debug("query %s", "select 1")
    app.warning("slow")
    db.error("failed: %s", "x")
    for i in range(10):
        app.info("event %d", i)
    logging.shutdown()
    for name in sorted(os.listdir(d)):
        print("==", name)
        print(open(os.path.join(d, name)).read(), end="")

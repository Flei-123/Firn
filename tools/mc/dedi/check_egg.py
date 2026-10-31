import json, re, sys, pathlib

egg_path = pathlib.Path(sys.argv[1])
egg = json.loads(egg_path.read_text())
errors = []

def need(cond, text):
    if not cond:
        errors.append(text)

for key in ("meta", "name", "author", "description", "docker_images", "startup", "config", "scripts", "variables", "file_denylist", "features", "exported_at"):
    need(key in egg, "missing key " + key)
need(egg["meta"].get("version") == "PTDL_v2", "meta.version must be PTDL_v2")
need(isinstance(egg["docker_images"], dict) and egg["docker_images"], "docker_images empty")
cfg = egg["config"]
for key in ("files", "startup", "logs", "stop"):
    need(key in cfg, "config." + key + " missing")
files = json.loads(cfg["files"])
startup_cfg = json.loads(cfg["startup"])
json.loads(cfg["logs"])
need(cfg["stop"] == "stop", "stop command must be stop")
need(startup_cfg.get("done") == "Done (", "done string must be 'Done ('")
script = egg["scripts"]["installation"]
need(script["script"].startswith("#!/bin/ash"), "install script needs an ash shebang")
need(script["container"].startswith("ghcr.io/pterodactyl/installers:"), "installer container")
need(script["entrypoint"] in ("ash", "bash"), "installer entrypoint")
envs = []
for v in egg["variables"]:
    for key in ("name", "description", "env_variable", "default_value", "user_viewable", "user_editable", "rules"):
        need(key in v, "variable field " + key)
    need(re.fullmatch(r"[A-Z][A-Z0-9_]*", v["env_variable"]) is not None, "bad env name " + v["env_variable"])
    need(v["rules"].strip() != "", "empty rules for " + v["env_variable"])
    need(isinstance(v["default_value"], str), "default must be a string: " + v["env_variable"])
    envs.append(v["env_variable"])
need(len(envs) == len(set(envs)), "duplicate env variables")
builtin = {"SERVER_PORT", "SERVER_IP", "SERVER_MEMORY"}
for name in re.findall(r"\{\{([A-Z0-9_]+)\}\}", egg["startup"]):
    need(name in envs or name in builtin, "startup references unknown variable " + name)
for fname, spec in files.items():
    need(spec.get("parser") == "properties", fname + ": parser")
    for k, val in spec["find"].items():
        for name in re.findall(r"\{\{server\.build\.env\.([A-Z0-9_]+)\}\}", val):
            need(name in envs, k + " references unknown variable " + name)
need("firnmc-server" in egg["startup"] or "BINARY_FILE" in egg["startup"], "startup does not start the server")
if errors:
    print("\n".join("  FAIL  egg: " + e for e in errors))
    sys.exit(1)
print("  ok    egg structure (%d variables, %d config files)" % (len(envs), len(files)))

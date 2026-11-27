import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const STATIC_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "static");

export function loadStatic(name) {
  const source = readFileSync(join(STATIC_DIR, name), "utf8");
  return import("data:text/javascript;base64," + Buffer.from(source).toString("base64"));
}

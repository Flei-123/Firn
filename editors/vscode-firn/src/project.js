const fs = require("fs");
const path = require("path");

const MANIFEST = "firn.package";

function findProjectDir(startPath, exists = fs.existsSync) {
  let dir = path.dirname(startPath);
  for (;;) {
    if (exists(path.join(dir, MANIFEST))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return undefined;
    dir = parent;
  }
}

function runTarget(filePath, exists = fs.existsSync) {
  return findProjectDir(filePath, exists) || filePath;
}

module.exports = { findProjectDir, runTarget, MANIFEST };

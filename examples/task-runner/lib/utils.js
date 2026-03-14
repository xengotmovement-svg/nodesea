const path = require("node:path");
const os = require("node:os");
const crypto = require("node:crypto");

function env() {
  return {
    platform: os.platform(),
    arch: os.arch(),
    node: process.version,
    cpus: os.cpus().length,
    mem: `${Math.round(os.totalmem() / 1024 / 1024)}MB`,
    pid: process.pid,
  };
}

function hash(data) {
  return crypto.createHash("sha256").update(data).digest("hex").slice(0, 12);
}

function formatTable(rows) {
  if (rows.length === 0) return "";
  const widths = rows[0].map((_, i) =>
    Math.max(...rows.map((r) => String(r[i]).length))
  );
  return rows
    .map((row) => row.map((cell, i) => String(cell).padEnd(widths[i])).join("  "))
    .join("\n");
}

module.exports = { env, hash, formatTable };

const { styleText } = require("node:util");

const LEVELS = { debug: 0, info: 1, warn: 2, error: 3 };

class Logger {
  constructor(prefix, level = "info") {
    this.prefix = prefix;
    this.level = LEVELS[level] ?? 1;
  }

  _log(level, color, ...args) {
    if (LEVELS[level] < this.level) return;
    const tag = styleText
      ? styleText(color, `[${this.prefix}]`)
      : `[${this.prefix}]`;
    const ts = new Date().toISOString().slice(11, 19);
    console.log(`${ts} ${tag}`, ...args);
  }

  debug(...args) { this._log("debug", "gray", ...args); }
  info(...args)  { this._log("info", "cyan", ...args); }
  warn(...args)  { this._log("warn", "yellow", ...args); }
  error(...args) { this._log("error", "red", ...args); }
}

module.exports = { Logger, LEVELS };

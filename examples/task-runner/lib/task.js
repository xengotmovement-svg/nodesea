const { Logger } = require("./logger");

class Task {
  constructor(name, fn, { deps = [], timeout = 5000 } = {}) {
    this.name = name;
    this.fn = fn;
    this.deps = deps;
    this.timeout = timeout;
    this.log = new Logger(name);
  }

  async run(ctx) {
    this.log.info("starting");
    const start = performance.now();

    const result = await Promise.race([
      this.fn(ctx, this.log),
      new Promise((_, reject) =>
        setTimeout(() => reject(new Error(`Task "${this.name}" timed out`)), this.timeout)
      ),
    ]);

    const elapsed = (performance.now() - start).toFixed(1);
    this.log.info(`done (${elapsed}ms)`);
    return result;
  }
}

module.exports = { Task };

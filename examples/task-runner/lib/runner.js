const { Logger } = require("./logger");

class Runner {
  constructor() {
    this.tasks = new Map();
    this.results = new Map();
    this.log = new Logger("runner");
  }

  register(task) {
    this.tasks.set(task.name, task);
  }

  async run(name) {
    if (this.results.has(name)) return this.results.get(name);

    const task = this.tasks.get(name);
    if (!task) throw new Error(`Unknown task: "${name}"`);

    // Run dependencies first (topological order).
    for (const dep of task.deps) {
      await this.run(dep);
    }

    const ctx = Object.fromEntries(this.results);
    const result = await task.run(ctx);
    this.results.set(name, result);
    return result;
  }

  async runAll(names) {
    this.log.info(`running ${names.length} task(s): ${names.join(", ")}`);
    const start = performance.now();

    for (const name of names) {
      await this.run(name);
    }

    const elapsed = (performance.now() - start).toFixed(1);
    this.log.info(`all tasks complete (${elapsed}ms)`);
    return Object.fromEntries(this.results);
  }
}

module.exports = { Runner };

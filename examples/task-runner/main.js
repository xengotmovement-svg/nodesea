// Task runner example — demonstrates bundling a complex module graph.
//
// Module graph:
//   main.js
//   ├── lib/runner.js
//   │   └── lib/logger.js
//   ├── lib/task.js
//   │   └── lib/logger.js  (shared)
//   ├── lib/utils.js
//   │   ├── node:path
//   │   ├── node:os
//   │   └── node:crypto
//   └── lib/logger.js      (shared)
//
// Build:  nodesea examples/task-runner/main.js
// Run:    ./main

const { Runner } = require("./lib/runner");
const { Task } = require("./lib/task");
const { env, hash, formatTable } = require("./lib/utils");
const { Logger } = require("./lib/logger");

const log = new Logger("main");

// --- Define tasks with dependencies ---

const gather = new Task("gather", async (_ctx, log) => {
  const info = env();
  log.info(`${info.platform}/${info.arch}, ${info.cpus} cpus, ${info.mem} RAM`);
  return info;
});

const checksum = new Task(
  "checksum",
  async (ctx, log) => {
    const data = JSON.stringify(ctx.gather);
    const h = hash(data);
    log.info(`env hash: ${h}`);
    return h;
  },
  { deps: ["gather"] }
);

const report = new Task(
  "report",
  async (ctx, log) => {
    const info = ctx.gather;
    const rows = [
      ["Field", "Value"],
      ["Platform", `${info.platform}/${info.arch}`],
      ["Node", info.node],
      ["CPUs", info.cpus],
      ["Memory", info.mem],
      ["PID", info.pid],
      ["Env Hash", ctx.checksum],
    ];
    log.info("report:\n" + formatTable(rows));
    return rows;
  },
  { deps: ["gather", "checksum"] }
);

// --- Run ---

async function main() {
  log.info("task-runner starting");

  const runner = new Runner();
  runner.register(gather);
  runner.register(checksum);
  runner.register(report);

  await runner.runAll(["report"]);
}

main().catch((err) => {
  log.error(err.message);
  process.exit(1);
});

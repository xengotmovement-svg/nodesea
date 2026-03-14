import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const runs = Number(process.env.BENCH_RUNS || 30);
const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
const resultsDir = path.resolve('results');
const jsonOut = path.join(resultsDir, `${timestamp}.json`);
const mdOut = path.join(resultsDir, `${timestamp}.md`);

function run(cmd, args) {
  const out = spawnSync(cmd, args, { stdio: 'inherit' });
  if (out.status !== 0) {
    throw new Error(`Command failed: ${cmd} ${args.join(' ')}`);
  }
}

async function exists(p) {
  try { await fs.access(p); return true; } catch { return false; }
}

async function main() {
  await fs.mkdir(resultsDir, { recursive: true });

  const cases = [];

  // nodesea cases (if built)
  if (await exists('./dist/nodesea')) {
    cases.push(['nodesea+code-cache', './dist/nodesea']);
  }
  if (await exists('./dist/nodesea-no-cc')) {
    cases.push(['nodesea', './dist/nodesea-no-cc']);
  }

  cases.push(
    ['node-sea', './dist/node-sea'],
    ['node-sea+code-cache', './dist/node-sea-code-cache'],
    ['bun-compile', './dist/bun-compile'],
    ['bun-compile+bytecode', './dist/bun-compile-bytecode']
  );

  const args = [
    '--warmup', '3',
    '--runs', String(runs),
    '--export-json', jsonOut,
    '--export-markdown', mdOut,
  ];
  for (const [name, cmd] of cases) {
    args.push('--command-name', name, cmd);
  }

  run('hyperfine', args);

  console.log(`Saved results:\n- ${jsonOut}\n- ${mdOut}`);
}

await main();

import { runWorkload } from './generated/index';

const started = performance.now();
const value = runWorkload();
const elapsedMs = performance.now() - started;

process.stdout.write(`READY value=${value} init_ms=${elapsedMs.toFixed(3)}\n`);

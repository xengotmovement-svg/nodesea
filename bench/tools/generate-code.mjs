import fs from 'node:fs/promises';
import path from 'node:path';

const moduleCount = Number(process.env.MODULE_COUNT || 500);
const funcsPerModule = Number(process.env.FUNCS_PER_MODULE || 14);
const payloadSize = Number(process.env.PAYLOAD_SIZE || 240);
const outDir = path.resolve('src/generated');

function lcg(seed) {
  let value = seed >>> 0;
  return () => {
    value = (value * 1664525 + 1013904223) >>> 0;
    return value / 0xffffffff;
  };
}

function makePayload(rand, len) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz0123456789';
  let s = '';
  for (let i = 0; i < len; i += 1) {
    s += alphabet[Math.floor(rand() * alphabet.length)];
  }
  return s;
}

function pickItem(rand, arr) {
  return arr[Math.floor(rand() * arr.length)];
}

// ---------------------------------------------------------------------------
// Function shape generators — each produces a structurally distinct function
// that takes (seed: number) => number so the caller contract stays the same.
// ---------------------------------------------------------------------------

function shapeLoopHash(rand, symbol, payloadSz) {
  const blob = makePayload(rand, payloadSz);
  return [
    `const _blob_${symbol} = '${blob}';`,
    `export function ${symbol}(seed: number): number {`,
    `  let x = seed;`,
    `  for (let k = 0; k < _blob_${symbol}.length; k++) {`,
    `    x = (x * 1664525 + _blob_${symbol}.charCodeAt(k) + 1013904223) | 0;`,
    `  }`,
    `  return x >>> 0;`,
    `}`,
  ];
}

function shapeReduceArray(rand, symbol, payloadSz) {
  const len = Math.max(8, Math.floor(rand() * payloadSz));
  const nums = Array.from({ length: len }, () => Math.floor(rand() * 0xffff));
  return [
    `const _arr_${symbol}: number[] = [${nums.join(',')}];`,
    `export function ${symbol}(seed: number): number {`,
    `  return _arr_${symbol}.reduce((acc, v, i) => {`,
    `    return ((acc ^ v) * 2654435761 + (seed >>> (i & 15))) >>> 0;`,
    `  }, seed) >>> 0;`,
    `}`,
  ];
}

function shapeSwitchCase(rand, symbol, payloadSz) {
  const cases = 4 + Math.floor(rand() * 12);
  const blob = makePayload(rand, payloadSz);
  const lines = [
    `const _sw_${symbol} = '${blob}';`,
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
    `  for (let i = 0; i < _sw_${symbol}.length; i++) {`,
    `    const c = _sw_${symbol}.charCodeAt(i) % ${cases};`,
    `    switch (c) {`,
  ];
  for (let c = 0; c < cases; c++) {
    const mult = Math.floor(rand() * 0xffff) | 1;
    const add = Math.floor(rand() * 0xffff);
    lines.push(`      case ${c}: h = (h * ${mult} + ${add}) >>> 0; break;`);
  }
  lines.push(
    `      default: h = (h ^ ${Math.floor(rand() * 0xffffff)}) >>> 0; break;`,
    `    }`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  );
  return lines;
}

function shapeClass(rand, symbol, payloadSz) {
  const className = `_C_${symbol}`;
  const fieldCount = 3 + Math.floor(rand() * 5);
  const fields = Array.from({ length: fieldCount }, (_, i) => ({
    name: `f${i}`,
    init: Math.floor(rand() * 0xffff),
  }));
  const blob = makePayload(rand, Math.floor(payloadSz / 2));
  const lines = [
    `class ${className} {`,
    ...fields.map((f) => `  ${f.name}: number;`),
    `  private _k = '${blob}';`,
    `  constructor(seed: number) {`,
    ...fields.map((f) => `    this.${f.name} = (seed ^ ${f.init}) >>> 0;`),
    `  }`,
    `  compute(): number {`,
    `    let h = 0;`,
    ...fields.map(
      (f) => `    h = (h * 31 + this.${f.name}) >>> 0;`
    ),
    `    for (let i = 0; i < this._k.length; i++) h = (h + this._k.charCodeAt(i)) >>> 0;`,
    `    return h;`,
    `  }`,
    `}`,
    `export function ${symbol}(seed: number): number {`,
    `  return new ${className}(seed).compute() >>> 0;`,
    `}`,
  ];
  return lines;
}

function shapeNestedLoops(rand, symbol, payloadSz) {
  const outer = 2 + Math.floor(rand() * 4);
  const inner = 3 + Math.floor(rand() * 6);
  const blob = makePayload(rand, payloadSz);
  return [
    `const _nl_${symbol} = '${blob}';`,
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
    `  for (let i = 0; i < ${outer}; i++) {`,
    `    for (let j = 0; j < ${inner}; j++) {`,
    `      const idx = (i * ${inner} + j) % _nl_${symbol}.length;`,
    `      h = (h * 2246822519 ^ _nl_${symbol}.charCodeAt(idx)) >>> 0;`,
    `    }`,
    `    h = (h + (h << 13)) >>> 0;`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  ];
}

function shapeMapLookup(rand, symbol) {
  const entryCount = 6 + Math.floor(rand() * 14);
  const keys = Array.from({ length: entryCount }, (_, i) => makePayload(rand, 6 + Math.floor(rand() * 10)));
  const lines = [
    `const _map_${symbol} = new Map<string, number>([`,
    ...keys.map((k) => `  ['${k}', ${Math.floor(rand() * 0xffffff)}],`),
    `]);`,
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
    `  for (const [k, v] of _map_${symbol}) {`,
    `    for (let i = 0; i < k.length; i++) h = (h ^ k.charCodeAt(i)) >>> 0;`,
    `    h = (h + v) >>> 0;`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  ];
  return lines;
}

function shapeBitManip(rand, symbol) {
  const steps = 6 + Math.floor(rand() * 10);
  const ops = ['<<', '>>>', '^', '+', '-', '*'];
  const lines = [
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
  ];
  for (let s = 0; s < steps; s++) {
    const op = pickItem(rand, ops);
    const operand =
      op === '<<' || op === '>>>'
        ? 1 + Math.floor(rand() * 24)
        : Math.floor(rand() * 0xffff) | 1;
    lines.push(`  h = (h ${op} ${operand}) >>> 0;`);
  }
  lines.push(`  return h >>> 0;`, `}`);
  return lines;
}

function shapeRecursiveLike(rand, symbol, payloadSz) {
  const blob = makePayload(rand, Math.floor(payloadSz / 2));
  const depth = 3 + Math.floor(rand() * 4);
  return [
    `const _rl_${symbol} = '${blob}';`,
    `export function ${symbol}(seed: number): number {`,
    `  const step = (h: number, d: number): number => {`,
    `    if (d <= 0) return h >>> 0;`,
    `    let v = h;`,
    `    for (let i = 0; i < _rl_${symbol}.length; i++) {`,
    `      v = (v * 31 + _rl_${symbol}.charCodeAt(i)) >>> 0;`,
    `    }`,
    `    return step(v ^ (d * 2654435761), d - 1);`,
    `  };`,
    `  return step(seed, ${depth});`,
    `}`,
  ];
}

function shapeObjectSpread(rand, symbol) {
  const propCount = 4 + Math.floor(rand() * 8);
  const props = Array.from({ length: propCount }, (_, i) => ({
    name: `p${i}`,
    val: Math.floor(rand() * 0xffff),
  }));
  return [
    `export function ${symbol}(seed: number): number {`,
    `  const base = { ${props.map((p) => `${p.name}: ${p.val}`).join(', ')} };`,
    `  const merged = { ...base, z: seed };`,
    `  let h = 0;`,
    `  for (const k in merged) {`,
    `    h = (h * 65599 + (merged as any)[k]) >>> 0;`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  ];
}

function shapeRegex(rand, symbol, payloadSz) {
  const blob = makePayload(rand, payloadSz);
  const charClass = pickItem(rand, ['[a-m]', '[n-z]', '[0-9]', '[a-z0-9]']);
  return [
    `const _rx_${symbol} = '${blob}';`,
    `const _re_${symbol} = /${charClass}/g;`,
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
    `  _re_${symbol}.lastIndex = 0;`,
    `  let m: RegExpExecArray | null;`,
    `  while ((m = _re_${symbol}.exec(_rx_${symbol})) !== null) {`,
    `    h = (h * 31 + m[0].charCodeAt(0)) >>> 0;`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  ];
}

function shapeTryCatch(rand, symbol, payloadSz) {
  const blob = makePayload(rand, Math.floor(payloadSz / 2));
  const thresh = Math.floor(rand() * 200) + 50;
  return [
    `const _tc_${symbol} = '${blob}';`,
    `export function ${symbol}(seed: number): number {`,
    `  let h = seed;`,
    `  for (let i = 0; i < _tc_${symbol}.length; i++) {`,
    `    try {`,
    `      const v = _tc_${symbol}.charCodeAt(i);`,
    `      if (v > ${thresh}) throw v;`,
    `      h = (h * 2246822519 + v) >>> 0;`,
    `    } catch (e) {`,
    `      h = (h ^ (e as number) * 1597334677) >>> 0;`,
    `    }`,
    `  }`,
    `  return h >>> 0;`,
    `}`,
  ];
}

function shapeClosureFactory(rand, symbol) {
  const steps = 3 + Math.floor(rand() * 5);
  const lines = [
    `export function ${symbol}(seed: number): number {`,
    `  const fns: Array<(x: number) => number> = [];`,
  ];
  for (let i = 0; i < steps; i++) {
    const mult = Math.floor(rand() * 0xffff) | 1;
    const add = Math.floor(rand() * 0xffff);
    lines.push(`  fns.push((x) => (x * ${mult} + ${add}) >>> 0);`);
  }
  lines.push(
    `  let h = seed;`,
    `  for (const f of fns) h = f(h);`,
    `  return h >>> 0;`,
    `}`,
  );
  return lines;
}

const shapes = [
  shapeLoopHash,
  shapeReduceArray,
  shapeSwitchCase,
  shapeClass,
  shapeNestedLoops,
  shapeMapLookup,
  shapeBitManip,
  shapeRecursiveLike,
  shapeObjectSpread,
  shapeRegex,
  shapeTryCatch,
  shapeClosureFactory,
];

// ---------------------------------------------------------------------------
// Main generator
// ---------------------------------------------------------------------------

async function main() {
  await fs.rm(outDir, { recursive: true, force: true });
  await fs.mkdir(outDir, { recursive: true });

  for (let i = 0; i < moduleCount; i += 1) {
    const rand = lcg(i + 1);
    const lines = [];
    lines.push(`// generated module ${i}`);

    for (let j = 0; j < funcsPerModule; j += 1) {
      const symbol = `fn_${i}_${j}`;
      const shape = shapes[Math.floor(rand() * shapes.length)];
      const fnLines = shape(rand, symbol, payloadSize);
      lines.push(...fnLines, '');
    }

    await fs.writeFile(path.join(outDir, `m${i}.ts`), lines.join('\n'));
  }

  // index.ts — imports all modules and runs every function
  const indexLines = [];
  for (let i = 0; i < moduleCount; i += 1) {
    indexLines.push(`import * as m${i} from './m${i}';`);
  }

  indexLines.push('');
  indexLines.push('export function runWorkload(): number {');
  indexLines.push('  let acc = 0;');
  for (let i = 0; i < moduleCount; i += 1) {
    for (let j = 0; j < funcsPerModule; j += 1) {
      indexLines.push(`  acc ^= m${i}.fn_${i}_${j}(${i + j});`);
    }
  }
  indexLines.push('  return acc >>> 0;');
  indexLines.push('}');

  await fs.writeFile(path.join(outDir, 'index.ts'), indexLines.join('\n'));

  console.log(
    `Generated ${moduleCount} modules, ${funcsPerModule} funcs/module, ${shapes.length} distinct shapes.`
  );
}

await main();

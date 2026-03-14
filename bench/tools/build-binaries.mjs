import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const distDir = path.resolve('dist');
const cacheDir = path.resolve('.cache');

function run(cmd, args, opts = {}) {
  const out = spawnSync(cmd, args, {
    stdio: 'inherit',
    ...opts
  });
  if (out.status !== 0) {
    throw new Error(`Command failed: ${cmd} ${args.join(' ')}`);
  }
}

async function ensure(filePath) {
  await fs.access(filePath);
}

async function buildNodeSea(inputFile, outName, seaBlobName, { useCodeCache = false } = {}) {
  const blobPath = path.join(cacheDir, seaBlobName);
  const seaConfigPath = path.join(cacheDir, `${outName}.sea-config.json`);
  const exePath = path.join(distDir, outName);

  const config = {
    main: path.resolve(inputFile),
    output: blobPath,
    disableExperimentalSEAWarning: true,
    ...(useCodeCache && { useCodeCache: true })
  };

  await fs.writeFile(seaConfigPath, JSON.stringify(config, null, 2));
  await fs.copyFile(process.execPath, exePath);

  run(process.execPath, ['--experimental-sea-config', seaConfigPath]);
  run('codesign', ['--remove-signature', exePath]);
  run(
    process.execPath,
    [
      path.resolve('node_modules/postject/dist/cli.js'),
      exePath,
      'NODE_SEA_BLOB',
      blobPath,
      '--sentinel-fuse',
      'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2',
      '--macho-segment-name',
      'NODE_SEA'
    ],
    { env: { ...process.env, POSTJECT_SENTINEL_FUSE: 'NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2' } }
  );
  run('codesign', ['--sign', '-', exePath]);
}

function findNodesea() {
  // Check NODESEA_BIN env, then repo's cargo build output, then PATH.
  if (process.env.NODESEA_BIN) return process.env.NODESEA_BIN;
  const repoBin = path.resolve('..', 'target', 'release', 'nodesea');
  try { fs.access(repoBin); return repoBin; } catch {}
  try {
    const out = spawnSync('which', ['nodesea'], { encoding: 'utf-8' });
    if (out.status === 0) return out.stdout.trim();
  } catch {}
  return null;
}

async function buildNodesea(outName, nodeseaBin, { noCodeCache = false } = {}) {
  const exePath = path.join(distDir, outName);
  const args = ['src/cli.ts', '-o', exePath];
  if (noCodeCache) args.push('--no-code-cache');
  run(nodeseaBin, args);
}

async function main() {
  await fs.mkdir(distDir, { recursive: true });
  await fs.mkdir(cacheDir, { recursive: true });

  await ensure(path.join(distDir, 'shared.bundle.cjs'));

  // nodesea (if available)
  const nodeseaBin = findNodesea();
  if (nodeseaBin) {
    console.log(`Using nodesea: ${nodeseaBin}`);
    await buildNodesea('nodesea', nodeseaBin);
    await buildNodesea('nodesea-no-cc', nodeseaBin, { noCodeCache: true });
  } else {
    console.log('nodesea not found, skipping nodesea builds (set NODESEA_BIN to override)');
  }

  await buildNodeSea('dist/shared.bundle.cjs', 'node-sea', 'node-sea.blob');
  await buildNodeSea('dist/shared.bundle.cjs', 'node-sea-code-cache', 'node-sea-code-cache.blob', { useCodeCache: true });

  run('bun', ['build', 'src/cli.ts', '--compile', '--minify', '--outfile', 'dist/bun-compile']);
  run('bun', ['build', 'src/cli.ts', '--compile', '--minify', '--bytecode', '--outfile', 'dist/bun-compile-bytecode']);

  const built = ['node-sea', 'node-sea-code-cache', 'bun-compile', 'bun-compile-bytecode'];
  if (nodeseaBin) built.unshift('nodesea', 'nodesea-no-cc');
  console.log(`Built binaries: ${built.map(b => 'dist/' + b).join(', ')}`);
}

await main();

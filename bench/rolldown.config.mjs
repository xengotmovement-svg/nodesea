import { defineConfig } from 'rolldown';

export default defineConfig([
  {
    input: 'src/cli.ts',
    output: [
      { file: 'dist/shared.bundle.mjs', format: 'esm', minify: true },
      { file: 'dist/shared.bundle.cjs', format: 'cjs', minify: true }
    ],
    platform: 'node',
    sourcemap: false,
    treeshake: false
  }
]);

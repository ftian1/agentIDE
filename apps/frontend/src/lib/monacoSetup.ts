/**
 * Monaco local bootstrap — load from pre-built AMD bundles in /vendor/monaco/.
 *
 * Previously we imported `monaco-editor` directly which forced Vite/Rollup to
 * process ~4000 Monaco modules on every production build (~40s of the 60s total).
 *
 * Now we use Monaco's own pre-built AMD bundles (copied from node_modules by
 * scripts/build-monaco.sh into public/vendor/monaco/).  @monaco-editor/react's
 * AMD loader fetches them at runtime — no bundling overhead.
 *
 * To upgrade monaco-editor: run `pnpm up monaco-editor`, then `scripts/build-monaco.sh`.
 */

import { loader } from '@monaco-editor/react';

// Point @monaco-editor/react at the local AMD bundles.
// The AMD loader will fetch /vendor/monaco/editor/editor.main.js etc.
loader.config({
  paths: { vs: '/vendor/monaco' },
});

// Pre-init so the first <Editor> mount doesn't race a network fetch.
loader.init().catch(() => {
  /* init is also driven by the first Editor mount; ignore double-init */
});

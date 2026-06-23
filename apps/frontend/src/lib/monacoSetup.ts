/**
 * Monaco local bootstrap.
 *
 * By default @monaco-editor/react fetches the entire Monaco engine (several MB)
 * from the jsdelivr CDN at first editor mount — slow over the network and
 * broken offline. This points the loader at the locally-bundled monaco-editor
 * instead, so opening a file is instant and works with no network.
 *
 * Importing this module for its side effect (before any <Editor> mounts) is
 * enough; main.tsx imports it eagerly.
 */
import { loader } from '@monaco-editor/react';
import * as monaco from 'monaco-editor';

// Wire Monaco's web workers to Vite's bundled worker URLs (Vite resolves the
// ?worker imports at build time, so they ship inside the app — no CDN).
import editorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker';
import jsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker';
import cssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker';
import htmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker';
import tsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
(self as any).MonacoEnvironment = {
  getWorker(_: string, label: string) {
    switch (label) {
      case 'json':
        return new jsonWorker();
      case 'css':
      case 'scss':
      case 'less':
        return new cssWorker();
      case 'html':
      case 'handlebars':
      case 'razor':
        return new htmlWorker();
      case 'typescript':
      case 'javascript':
        return new tsWorker();
      default:
        return new editorWorker();
    }
  },
};

// Use the local monaco instance instead of the CDN AMD loader.
loader.config({ monaco });

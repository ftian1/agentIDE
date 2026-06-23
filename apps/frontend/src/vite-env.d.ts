/// <reference types="vite/client" />

// Vite's `?worker` import suffix returns a Worker constructor. Declared here so
// the Monaco worker imports in monacoSetup.ts typecheck under tsc.
declare module '*?worker' {
  const workerConstructor: {
    new (): Worker;
  };
  export default workerConstructor;
}

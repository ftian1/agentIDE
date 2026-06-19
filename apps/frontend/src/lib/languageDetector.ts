/** Map file extension to Monaco editor language ID. */
const EXT_TO_LANG: Record<string, string> = {
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  rs: 'rust',
  py: 'python',
  pyi: 'python',
  go: 'go',
  java: 'java',
  c: 'c',
  h: 'c',
  cpp: 'cpp',
  cc: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  rb: 'ruby',
  php: 'php',
  swift: 'swift',
  kt: 'kotlin',
  scala: 'scala',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  yml: 'yaml',
  yaml: 'yaml',
  json: 'json',
  xml: 'xml',
  html: 'html',
  htm: 'html',
  css: 'css',
  scss: 'scss',
  less: 'less',
  sql: 'sql',
  md: 'markdown',
  markdown: 'markdown',
  toml: 'ini',
  ini: 'ini',
  cfg: 'ini',
  env: 'plaintext',
  txt: 'plaintext',
  log: 'plaintext',
  diff: 'diff',
  patch: 'diff',
  lock: 'plaintext',
  gitignore: 'plaintext',
  dockerfile: 'dockerfile',
  makefile: 'makefile',
  cmake: 'cmake',
};

export function detectLanguage(filePath: string): string {
  const parts = filePath.split('/');
  const filename = parts[parts.length - 1].toLowerCase();

  // Special filenames
  if (filename === 'dockerfile') return 'dockerfile';
  if (filename === 'makefile') return 'makefile';
  if (filename === 'cmakelists.txt') return 'cmake';
  if (filename === '.gitignore') return 'plaintext';
  if (filename === '.env') return 'plaintext';
  if (filename.startsWith('.')) {
    // Hidden config files — try extension after first dot
    const ext = filename.split('.').pop();
    if (ext && EXT_TO_LANG[ext]) return EXT_TO_LANG[ext];
  }

  // Extension-based
  const ext = filename.includes('.') ? filename.split('.').pop() : null;
  if (ext && EXT_TO_LANG[ext]) return EXT_TO_LANG[ext];

  return 'plaintext';
}

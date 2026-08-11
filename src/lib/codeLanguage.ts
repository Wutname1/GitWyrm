import type { Extension } from '@codemirror/state'

/**
 * Picks a CodeMirror language for a file path, for the read-only Raw view.
 *
 * Every language is imported dynamically. Together they are several hundred
 * kilobytes of parser, and a session that opens one Rust file has no reason to
 * carry the PHP and SQL grammars with it. The Raw view is itself lazy-loaded,
 * so the language arrives alongside the editor rather than after it.
 *
 * A file we have no grammar for is not an error: it renders as plain monospace
 * text with line numbers, which is still the thing the user asked to see.
 */

/**
 * Languages with a first-party `@codemirror/lang-*` package. These get full
 * incremental Lezer parsers.
 */
const MODERN: Record<string, () => Promise<Extension>> = {
  javascript: async () => (await import('@codemirror/lang-javascript')).javascript(),
  jsx: async () => (await import('@codemirror/lang-javascript')).javascript({ jsx: true }),
  typescript: async () =>
    (await import('@codemirror/lang-javascript')).javascript({ typescript: true }),
  tsx: async () =>
    (await import('@codemirror/lang-javascript')).javascript({ typescript: true, jsx: true }),
  rust: async () => (await import('@codemirror/lang-rust')).rust(),
  json: async () => (await import('@codemirror/lang-json')).json(),
  markdown: async () => (await import('@codemirror/lang-markdown')).markdown(),
  html: async () => (await import('@codemirror/lang-html')).html(),
  css: async () => (await import('@codemirror/lang-css')).css(),
  python: async () => (await import('@codemirror/lang-python')).python(),
  xml: async () => (await import('@codemirror/lang-xml')).xml(),
  yaml: async () => (await import('@codemirror/lang-yaml')).yaml(),
  java: async () => (await import('@codemirror/lang-java')).java(),
  cpp: async () => (await import('@codemirror/lang-cpp')).cpp(),
  php: async () => (await import('@codemirror/lang-php')).php(),
  sql: async () => (await import('@codemirror/lang-sql')).sql(),
  go: async () => (await import('@codemirror/lang-go')).go(),
}

/**
 * Languages served by the CodeMirror 5 mode shims. These are stream parsers
 * rather than full grammars -- highlighting is per-line and slightly coarser --
 * but they cover a long tail of languages that have no modern package, and a
 * coarse colouring reads far better than none.
 */
const LEGACY: Record<string, () => Promise<Extension>> = {
  csharp: async () => {
    const [{ StreamLanguage }, { csharp }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/clike'),
    ])
    return StreamLanguage.define(csharp)
  },
  ruby: async () => {
    const [{ StreamLanguage }, { ruby }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/ruby'),
    ])
    return StreamLanguage.define(ruby)
  },
  lua: async () => {
    const [{ StreamLanguage }, { lua }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/lua'),
    ])
    return StreamLanguage.define(lua)
  },
  shell: async () => {
    const [{ StreamLanguage }, { shell }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/shell'),
    ])
    return StreamLanguage.define(shell)
  },
  powershell: async () => {
    const [{ StreamLanguage }, { powerShell }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/powershell'),
    ])
    return StreamLanguage.define(powerShell)
  },
  toml: async () => {
    const [{ StreamLanguage }, { toml }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/toml'),
    ])
    return StreamLanguage.define(toml)
  },
  ini: async () => {
    const [{ StreamLanguage }, { properties }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/properties'),
    ])
    return StreamLanguage.define(properties)
  },
  swift: async () => {
    const [{ StreamLanguage }, { swift }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/swift'),
    ])
    return StreamLanguage.define(swift)
  },
  kotlin: async () => {
    const [{ StreamLanguage }, { kotlin }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/clike'),
    ])
    return StreamLanguage.define(kotlin)
  },
  diff: async () => {
    const [{ StreamLanguage }, { diff }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/diff'),
    ])
    return StreamLanguage.define(diff)
  },
  dockerfile: async () => {
    const [{ StreamLanguage }, { dockerFile }] = await Promise.all([
      import('@codemirror/language'),
      import('@codemirror/legacy-modes/mode/dockerfile'),
    ])
    return StreamLanguage.define(dockerFile)
  },
}

const LOADERS: Record<string, () => Promise<Extension>> = { ...MODERN, ...LEGACY }

/** A language we have a grammar for. */
type LanguageKey = string

/** Extension (lowercase, no dot) to language key. */
const BY_EXTENSION: Record<string, LanguageKey> = {
  js: 'javascript',
  mjs: 'javascript',
  cjs: 'javascript',
  jsx: 'jsx',
  ts: 'typescript',
  mts: 'typescript',
  cts: 'typescript',
  tsx: 'tsx',
  rs: 'rust',
  json: 'json',
  jsonc: 'json',
  md: 'markdown',
  mdx: 'markdown',
  markdown: 'markdown',
  html: 'html',
  htm: 'html',
  vue: 'html',
  svelte: 'html',
  css: 'css',
  scss: 'css',
  less: 'css',
  py: 'python',
  pyi: 'python',
  xml: 'xml',
  svg: 'xml',
  xaml: 'xml',
  plist: 'xml',
  yaml: 'yaml',
  yml: 'yaml',
  java: 'java',
  c: 'cpp',
  h: 'cpp',
  cc: 'cpp',
  cpp: 'cpp',
  cxx: 'cpp',
  hpp: 'cpp',
  hh: 'cpp',
  m: 'cpp',
  mm: 'cpp',
  php: 'php',
  sql: 'sql',
  go: 'go',
  cs: 'csharp',
  rb: 'ruby',
  rake: 'ruby',
  gemspec: 'ruby',
  lua: 'lua',
  sh: 'shell',
  bash: 'shell',
  zsh: 'shell',
  fish: 'shell',
  ps1: 'powershell',
  psm1: 'powershell',
  psd1: 'powershell',
  toml: 'toml',
  ini: 'ini',
  cfg: 'ini',
  conf: 'ini',
  properties: 'ini',
  env: 'ini',
  swift: 'swift',
  kt: 'kotlin',
  kts: 'kotlin',
  patch: 'diff',
  diff: 'diff',
}

/**
 * Files that carry their language in the name rather than an extension.
 * Matched on the lowercased file name, so `Dockerfile` and `dockerfile` both
 * land, and `Dockerfile.prod` is handled by the prefix check below.
 */
const BY_FILENAME: Record<string, LanguageKey> = {
  dockerfile: 'dockerfile',
  containerfile: 'dockerfile',
  makefile: 'shell',
  gnumakefile: 'shell',
  rakefile: 'ruby',
  gemfile: 'ruby',
  '.gitignore': 'ini',
  '.gitattributes': 'ini',
  '.editorconfig': 'ini',
  '.env': 'ini',
  '.npmrc': 'ini',
  '.babelrc': 'json',
  '.eslintrc': 'json',
  '.prettierrc': 'json',
  'cargo.lock': 'toml',
  'go.mod': 'go',
  'go.sum': 'ini',
}

/**
 * Name of the language a path will be shown in, for the header badge. Null when
 * nothing matches, which the caller shows as plain text.
 */
export function languageLabel(path: string): string | null {
  const key = languageKey(path)
  if (!key) return null
  return LABELS[key] ?? key
}

const LABELS: Record<LanguageKey, string> = {
  javascript: 'JavaScript',
  jsx: 'JavaScript',
  typescript: 'TypeScript',
  tsx: 'TypeScript',
  rust: 'Rust',
  json: 'JSON',
  markdown: 'Markdown',
  html: 'HTML',
  css: 'CSS',
  python: 'Python',
  xml: 'XML',
  yaml: 'YAML',
  java: 'Java',
  cpp: 'C / C++',
  php: 'PHP',
  sql: 'SQL',
  go: 'Go',
  csharp: 'C#',
  ruby: 'Ruby',
  lua: 'Lua',
  shell: 'Shell',
  powershell: 'PowerShell',
  toml: 'TOML',
  ini: 'INI',
  swift: 'Swift',
  kotlin: 'Kotlin',
  diff: 'Diff',
  dockerfile: 'Dockerfile',
}

function languageKey(path: string): LanguageKey | null {
  const name = (path.split(/[\\/]/).pop() ?? '').toLowerCase()
  if (!name) return null

  const byName = BY_FILENAME[name]
  if (byName) return byName
  // `Dockerfile.prod`, `Makefile.local`: the language is in the stem.
  const stem = name.split('.')[0]
  if (stem && BY_FILENAME[stem]) return BY_FILENAME[stem]

  // Walk suffixes from longest to shortest so `.d.ts` and `.spec.tsx` still
  // resolve through their final extension.
  const ext = name.includes('.') ? name.slice(name.lastIndexOf('.') + 1) : ''
  return BY_EXTENSION[ext] ?? null
}

/**
 * Load the language extension for a path, or null when we have no grammar for
 * it. A loader that fails to import resolves to null rather than throwing: a
 * missing grammar should cost the colours, not the file.
 */
export async function loadLanguage(path: string): Promise<Extension | null> {
  const key = languageKey(path)
  if (!key) return null
  try {
    return await LOADERS[key]()
  } catch {
    return null
  }
}

import { readdirSync, statSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, basename } from 'node:path';

const root = 'src/components';
const files = [];
// dev/ holds dev-only tooling (ThemeLab etc.), excluded from the DS export.
const EXCLUDE_DIRS = new Set(['dev']);
(function walk(d) {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    const s = statSync(p);
    if (s.isDirectory()) {
      if (EXCLUDE_DIRS.has(n)) continue;
      walk(p);
    } else if (/\.tsx$/.test(n)) files.push(p);
  }
})(root);

// Collect every PascalCase value export across component files.
const declRx = /export\s+(?:default\s+)?(?:const|let|var|function|class)\s+([A-Z][A-Za-z0-9]*)/g;
const braceRx = /export\s*\{([^}]*)\}/g;

const map = {};
const collisions = [];
for (const f of files) {
  if (basename(f) === '_ds_barrel.ts') continue;
  const txt = readFileSync(f, 'utf8');
  const rel = './' + relative('.', f).split('\\').join('/');
  const names = new Set();
  let m;
  while ((m = declRx.exec(txt))) names.add(m[1]);
  while ((m = braceRx.exec(txt))) {
    for (let raw of m[1].split(',')) {
      raw = raw.trim();
      if (!raw) continue;
      // handle `Foo as Bar`
      const asMatch = raw.match(/\bas\s+([A-Za-z0-9_]+)$/);
      const exported = asMatch ? asMatch[1] : raw;
      if (/^[A-Z][A-Za-z0-9]*$/.test(exported)) names.add(exported);
    }
  }
  for (const name of names) {
    if (map[name] && map[name] !== rel) collisions.push([name, map[name], rel]);
    else map[name] = rel;
  }
}

const sorted = Object.fromEntries(Object.entries(map).sort((a, b) => a[0].localeCompare(b[0])));
writeFileSync('.ds-sync/srcmap.json', JSON.stringify(sorted, null, 2) + '\n');
console.log('components:', Object.keys(sorted).length);
if (collisions.length) {
  console.log('COLLISIONS:');
  for (const [n, a, b] of collisions) console.log(`  ${n}: ${a} vs ${b}`);
}
console.log(Object.keys(sorted).join(', '));

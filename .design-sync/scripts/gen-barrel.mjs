import { readdirSync, statSync, writeFileSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = 'src/components';
const files = [];
(function walk(d) {
  for (const n of readdirSync(d)) {
    const p = join(d, n);
    const s = statSync(p);
    if (s.isDirectory()) walk(p);
    else if (/\.tsx$/.test(n)) files.push(p);
  }
})(root);

const exportRx = /export\s+(?:default\s+)?(?:const|let|var|function|class)\s+([A-Z][A-Za-z0-9]*)|export\s*\{([^}]*)\}/g;
const keep = [];
for (const f of files) {
  if (f.endsWith('_ds_barrel.ts')) continue;
  const txt = readFileSync(f, 'utf8');
  let m, has = false;
  while ((m = exportRx.exec(txt))) {
    if (m[1] && /^[A-Z]/.test(m[1])) has = true;
    if (m[2] && /[A-Z][A-Za-z0-9]*/.test(m[2])) has = true;
  }
  if (has) keep.push(f);
}

const lines = keep.map((f) => {
  const rel = relative('src/components', f).split('\\').join('/').replace(/\.tsx$/, '');
  return `export * from './${rel}';`;
});
writeFileSync('src/components/_ds_barrel.ts', lines.join('\n') + '\n');
console.log('barrel entries:', lines.length);

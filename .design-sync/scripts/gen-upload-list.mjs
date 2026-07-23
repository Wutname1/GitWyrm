// Build the list of ds-bundle-relative file paths to upload for a given set of
// component names, plus the shared base files. Writes newline-separated paths
// to .design-sync/.cache/upload/<label>.txt.
import { readdirSync, statSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const [, , label, ...names] = process.argv;
const includeBase = names[0] === '--base';
const compNames = new Set(includeBase ? names.slice(1) : names);

const ROOT = 'ds-bundle';
const files = [];

// Shared base files (first push carries these).
if (includeBase) {
  for (const f of ['_ds_bundle.js', '_ds_bundle.css', 'styles.css', 'README.md', '_ds_sync.json']) {
    if (existsSync(join(ROOT, f))) files.push(f);
  }
  for (const dir of ['_vendor', 'fonts', 'tokens', 'guidelines']) {
    const d = join(ROOT, dir);
    if (!existsSync(d)) continue;
    (function walk(p) {
      for (const n of readdirSync(p)) {
        const fp = join(p, n);
        if (statSync(fp).isDirectory()) walk(fp);
        else files.push(relative(ROOT, fp).split('\\').join('/'));
      }
    })(d);
  }
}

// Per-component dirs + their _preview files.
(function walkComponents(p) {
  for (const n of readdirSync(p)) {
    const fp = join(p, n);
    if (statSync(fp).isDirectory()) walkComponents(fp);
    else {
      // components/<group>/<Name>/<Name>.<ext> — component name is the dir.
      const rel = relative(ROOT, fp).split('\\').join('/');
      const m = rel.match(/^components\/[^/]+\/([^/]+)\//);
      if (m && compNames.has(m[1])) files.push(rel);
    }
  }
})(join(ROOT, 'components'));

// _preview/<Name>.* for authored components in the set.
const prevDir = join(ROOT, '_preview');
if (existsSync(prevDir)) {
  for (const n of readdirSync(prevDir)) {
    const base = n.replace(/\.[^.]+$/, '');
    if (compNames.has(base)) files.push(`_preview/${n}`);
  }
}

const out = join('.design-sync/.cache/upload', `${label}.txt`);
writeFileSync(out, files.join('\n') + '\n');
console.log(`${label}: ${files.length} files -> ${out}`);

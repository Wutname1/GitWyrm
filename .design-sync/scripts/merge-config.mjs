import { readFileSync, writeFileSync } from 'node:fs';

const cfg = JSON.parse(readFileSync('.design-sync/config.json', 'utf8'));
const srcmap = JSON.parse(readFileSync('.ds-sync/srcmap.json', 'utf8'));

// Preserve any existing null exclusions / pins the user or prior run set.
const existing = cfg.componentSrcMap ?? {};
const merged = { ...srcmap };
for (const [k, v] of Object.entries(existing)) merged[k] = v; // existing wins (null excludes, or pin override)

cfg.componentSrcMap = Object.fromEntries(Object.entries(merged).sort((a, b) => a[0].localeCompare(b[0])));
writeFileSync('.design-sync/config.json', JSON.stringify(cfg, null, 2) + '\n');
console.log('componentSrcMap entries:', Object.keys(cfg.componentSrcMap).length);

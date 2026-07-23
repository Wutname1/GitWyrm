// Stage the Vite-compiled stylesheet for the converter.
// Vite emits a content-hashed dist/assets/index-<hash>.css with expanded
// Tailwind utilities, resolved --gw-* tokens, and @font-face rules whose
// url()s are ROOT-absolute (/assets/*.woff2). The converter's extractFonts
// resolves url()s relative to the CSS file, so rewrite /assets/ -> ./assets/
// and copy to a stable path (dist/gitwyrm.css) that cfg.cssEntry points at.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const assets = 'dist/assets';
const css = readdirSync(assets).filter((f) => /^index-.*\.css$/.test(f));
if (css.length !== 1) {
  console.error(`expected exactly one dist/assets/index-*.css, found ${css.length}: ${css.join(', ')}`);
  process.exit(1);
}
const srcPath = join(assets, css[0]);
let text = readFileSync(srcPath, 'utf8');
// Root-absolute asset urls -> relative to dist/ (where the staged css lives).
text = text.replace(/url\(\/assets\//g, 'url(./assets/');
writeFileSync('dist/gitwyrm.css', text);
console.log(`staged dist/gitwyrm.css from ${css[0]} (${text.length} bytes)`);

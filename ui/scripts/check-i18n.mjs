// 語系檔完整性檢查：
//  1. 前端 TypeScript 中的 t("...") 鍵值必須存在於每個語系檔。
//  2. Rust 後端 (crates / src-tauri) 內含中文的字串，必須存在於語系檔的 "_backend" 區塊。
//     後端文字中的 {} / {0} / {:.1} 佔位符一律正規化為 {}。
// 用法：node scripts/check-i18n.mjs [--dump-backend]
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const uiRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const src = path.join(uiRoot, 'src');
const repoRoot = path.resolve(uiRoot, '..');
const CJK = /[\u4e00-\u9fff]/;

// ---- 前端鍵值 ----
const used = new Set();
for (const f of walk(src, [], /\.(ts|tsx)$/)) {
  const text = fs.readFileSync(f, 'utf8');
  for (const m of text.matchAll(/\bt\(("(?:[^"\\]|\\.)*")/g)) used.add(JSON.parse(m[1]));
}

// ---- 後端鍵值 ----
function walk(dir, out = [], extension = /\.rs$/) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === 'target' || e.name === 'node_modules') continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out, extension);
    else if (extension.test(e.name)) out.push(p);
  }
  return out;
}
const backend = new Set();
for (const dir of ['crates', 'src-tauri']) {
  const d = path.join(repoRoot, dir);
  if (!fs.existsSync(d)) continue;
  for (const f of walk(d)) {
    const lines = fs.readFileSync(f, 'utf8').split(/\r?\n/);
    for (const line of lines) {
      const t = line.trim();
      if (!CJK.test(t) || t.startsWith('//') || /assert|name_lower\s*==|lower\s*==/.test(t)) continue;
      for (const m of line.matchAll(/"((?:[^"\\]|\\.)*)"/g)) {
        if (!CJK.test(m[1])) continue;
        const text = m[1].replace(/\\(["\\])/g, '$1').replace(/\{[^}]*\}/g, '{}');
        backend.add(text);
      }
    }
  }
}

if (process.argv.includes('--dump-backend')) {
  fs.writeFileSync(path.join(uiRoot, 'scripts', 'backend-keys.json'), JSON.stringify([...backend], null, 1));
  console.log('dumped', backend.size);
  process.exit(0);
}

let failed = false;
const dir = path.join(src, 'locales');
for (const name of fs.readdirSync(dir).filter((n) => n.endsWith('.json'))) {
  const dict = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
  const be = dict._backend ?? {};
  const isSource = name.startsWith('zh-TW');
  const missing = [...used].filter((k) => !(k in dict));
  const missingBe = isSource ? [] : [...backend].filter((k) => !(k in be));
  const extra = Object.keys(dict).filter((k) => k !== '_meta' && k !== '_backend' && !used.has(k));
  if (missing.length) {
    failed = true;
    console.error(`[${name}] 缺少 ${missing.length} 個前端翻譯：`, missing);
  }
  if (missingBe.length) {
    failed = true;
    console.error(`[${name}] 缺少 ${missingBe.length} 個後端翻譯：`, missingBe);
  }
  if (extra.length) console.warn(`[${name}] 有 ${extra.length} 個未使用的鍵值：`, extra);
  if (!dict._meta?.name) {
    failed = true;
    console.error(`[${name}] 缺少 _meta.name`);
  }
  console.log(
    `[${name}] 前端 ${used.size - missing.length}/${used.size}` +
      (isSource ? '' : `，後端 ${backend.size - missingBe.length}/${backend.size}`),
  );
}
process.exit(failed ? 1 : 0);

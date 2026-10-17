// 檢查 App 與 lib 中的 t("...") 鍵值是否都存在於每個語系檔，並列出多餘鍵值。
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'src');
const files = ['App.tsx', 'lib/bridge.ts'].map((f) => path.join(root, f));
const used = new Set();
for (const f of files) {
  const text = fs.readFileSync(f, 'utf8');
  for (const m of text.matchAll(/\bt\(("(?:[^"\\]|\\.)*")/g)) used.add(JSON.parse(m[1]));
}

let failed = false;
const dir = path.join(root, 'locales');
for (const name of fs.readdirSync(dir).filter((n) => n.endsWith('.json'))) {
  const dict = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
  const missing = [...used].filter((k) => !(k in dict));
  const extra = Object.keys(dict).filter((k) => k !== '_meta' && !used.has(k));
  if (missing.length) {
    failed = true;
    console.error(`[${name}] 缺少 ${missing.length} 個翻譯：`, missing);
  }
  if (extra.length) console.warn(`[${name}] 有 ${extra.length} 個未使用的鍵值：`, extra);
  if (!dict._meta?.name) {
    failed = true;
    console.error(`[${name}] 缺少 _meta.name`);
  }
  console.log(`[${name}] OK: ${used.size - missing.length}/${used.size}`);
}
process.exit(failed ? 1 : 0);

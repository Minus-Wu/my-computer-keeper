// 多語系模組：自動偵測 src/locales/*.json 語系檔，依使用者偏好 / 瀏覽器語系決定介面語言。
// 翻譯鍵值為繁體中文原文；找不到對應翻譯時直接顯示原文。
type Dict = Record<string, unknown>;

const modules = import.meta.glob('./locales/*.json', {
  eager: true,
  import: 'default',
}) as Record<string, Dict>;

export interface LocaleInfo {
  code: string;
  name: string;
}

const dictionaries: Record<string, Record<string, string>> = {};
// 後端 (Rust) 回傳的中文訊息翻譯，鍵值中的 {} 為動態參數佔位符。
interface BackendDict {
  exact: Record<string, string>;
  patterns: { re: RegExp; text: string }[];
}
const backendDicts: Record<string, BackendDict> = {};
const localeList: LocaleInfo[] = [];

for (const [path, dict] of Object.entries(modules)) {
  const code = path.replace(/^.*\/(.+)\.json$/, '$1');
  const meta = dict._meta as { name?: string } | undefined;
  const strings: Record<string, string> = {};
  for (const [k, v] of Object.entries(dict)) {
    if (k !== '_meta' && typeof v === 'string') strings[k] = v;
  }
  dictionaries[code] = strings;
  const backend: BackendDict = { exact: {}, patterns: [] };
  const rawBackend = (dict._backend ?? {}) as Record<string, unknown>;
  for (const [k, v] of Object.entries(rawBackend)) {
    if (typeof v !== 'string') continue;
    if (k.includes('{}')) {
      const source = k
        .split('{}')
        .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
        .join('([\\s\\S]*?)');
      backend.patterns.push({ re: new RegExp('^' + source + '$'), text: v });
    } else {
      backend.exact[k] = v;
    }
  }
  backendDicts[code] = backend;
  localeList.push({ code, name: meta?.name ?? code });
}

localeList.sort((a, b) => a.code.localeCompare(b.code));

export const availableLocales: LocaleInfo[] = localeList;
export const DEFAULT_LOCALE = dictionaries['en'] ? 'en' : (localeList[0]?.code ?? 'zh-TW');

const STORAGE_KEY = 'mck.locale';

/** 將瀏覽器語系標籤（如 zh-Hant-TW、ja-JP、en-US）對應到已有的語系檔。 */
function matchLocale(tag: string): string | null {
  const lower = tag.toLowerCase();
  const codes = localeList.map((l) => l.code);
  const exact = codes.find((c) => c.toLowerCase() === lower);
  if (exact) return exact;
  const base = lower.split('-')[0];
  if (base === 'zh') {
    // 繁體系（TW/HK/MO/Hant）優先對應 zh-TW；其他中文也退而使用既有的 zh 語系檔
    return codes.find((c) => c.toLowerCase().startsWith('zh')) ?? null;
  }
  return codes.find((c) => c.toLowerCase().split('-')[0] === base) ?? null;
}

export function detectLocale(): string {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && dictionaries[saved]) return saved;
  } catch {
    /* localStorage 不可用時忽略 */
  }
  const tags =
    typeof navigator !== 'undefined'
      ? navigator.languages?.length
        ? navigator.languages
        : [navigator.language]
      : [];
  for (const tag of tags) {
    const hit = tag ? matchLocale(tag) : null;
    if (hit) return hit;
  }
  return DEFAULT_LOCALE;
}

export function persistLocale(code: string) {
  try {
    localStorage.setItem(STORAGE_KEY, code);
  } catch {
    /* 忽略 */
  }
}

export type TranslateParams = Record<string, string | number>;

export function translate(locale: string, key: string, params?: TranslateParams): string {
  const text = dictionaries[locale]?.[key] ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) =>
    name in params ? String(params[name]) : m,
  );
}

/**
 * 翻譯後端 (Rust) 回傳的中文訊息：先精確比對，再比對含 {} 參數的樣式。
 * 找不到對應翻譯（例如檔名、路徑、系統訊息）時原樣回傳。
 */
export function translateBackend(locale: string, text: string | null | undefined, depth = 0): string {
  if (!text) return text ?? '';
  const dict = backendDicts[locale];
  if (!dict) return text;
  const exact = dict.exact[text];
  if (exact !== undefined) return exact;
  if (depth > 2 || !/[\u4e00-\u9fff]/.test(text)) return text;
  for (const { re, text: tpl } of dict.patterns) {
    const m = re.exec(text);
    if (!m) continue;
    const args = m.slice(1).map((a) => translateBackend(locale, a, depth + 1));
    let i = 0;
    return tpl.replace(/\{\}/g, () => args[i++] ?? '');
  }
  return text;
}

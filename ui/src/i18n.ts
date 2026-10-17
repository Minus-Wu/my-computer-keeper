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
const localeList: LocaleInfo[] = [];

for (const [path, dict] of Object.entries(modules)) {
  const code = path.replace(/^.*\/(.+)\.json$/, '$1');
  const meta = dict._meta as { name?: string } | undefined;
  const strings: Record<string, string> = {};
  for (const [k, v] of Object.entries(dict)) {
    if (k !== '_meta' && typeof v === 'string') strings[k] = v;
  }
  dictionaries[code] = strings;
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

import { useMemo, useState } from 'react';
import { ArrowDown, ArrowUp, Settings2 } from 'lucide-react';
import { formatBytes, formatUptime } from '../lib/bridge';
import type { TranslateParams } from '../i18n';
import type { ProcessEntry, SystemSnapshot } from '../types/keeper';

type ColumnId = keyof ProcessEntry | 'memory_percent';
type Translate = (key: string, params?: TranslateParams) => string;
type Column = {
  id: ColumnId;
  label: string;
  numeric?: boolean;
  value: (process: ProcessEntry) => string | number | null;
  format?: (value: string | number) => string;
};

const STORAGE_KEY = 'keeper.taskmgr.columns.v1';
const DEFAULT_COLUMNS: ColumnId[] = ['pid', 'name', 'cpu_usage', 'memory_bytes', 'exe_path'];

function readColumns(): { ids: string[]; failed: boolean } {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === null) return { ids: DEFAULT_COLUMNS, failed: false };
    const ids: unknown = JSON.parse(saved);
    if (!Array.isArray(ids) || ids.length === 0 || !ids.every((id) => typeof id === 'string')) {
      throw new Error('Invalid process column preferences');
    }
    return { ids, failed: false };
  } catch (error) {
    console.warn('Could not read process column preferences:', error);
    return { ids: DEFAULT_COLUMNS, failed: true };
  }
}

export function ProcessTable({
  snapshot, search, loading, onKill, t,
}: {
  snapshot: SystemSnapshot | null;
  search: string;
  loading: boolean;
  onKill: (pid: number) => void;
  t: Translate;
}) {
  const [preferences, setPreferences] = useState(readColumns);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [saveFailed, setSaveFailed] = useState(false);
  const [sort, setSort] = useState<{ id: ColumnId; descending: boolean }>({
    id: 'memory_bytes', descending: true,
  });
  const columns = useMemo<Column[]>(() => {
    const bytes = (value: string | number) => formatBytes(Number(value));
    const percent = (value: string | number) => `${Number(value).toFixed(1)}%`;
    const rate = (value: string | number) => `${formatBytes(Number(value))}/s`;
    return [
      { id: 'pid', label: 'PID', numeric: true, value: (p) => p.pid },
      { id: 'name', label: t("處理程序名稱"), value: (p) => p.name },
      { id: 'parent_pid', label: t("父處理程序 PID"), numeric: true, value: (p) => p.parent_pid },
      { id: 'cpu_usage', label: 'CPU %', numeric: true, value: (p) => p.cpu_usage, format: percent },
      { id: 'memory_bytes', label: t("記憶體"), numeric: true, value: (p) => p.memory_bytes, format: bytes },
      { id: 'memory_percent', label: t("記憶體 %"), numeric: true, value: (p) => snapshot?.total_memory_bytes ? p.memory_bytes / snapshot.total_memory_bytes * 100 : null, format: percent },
      { id: 'virtual_memory_bytes', label: t("虛擬記憶體"), numeric: true, value: (p) => p.virtual_memory_bytes, format: bytes },
      { id: 'gpu_usage', label: 'GPU %', numeric: true, value: (p) => p.gpu_usage, format: percent },
      { id: 'gpu_memory_bytes', label: t("GPU 記憶體"), numeric: true, value: (p) => p.gpu_memory_bytes, format: bytes },
      { id: 'gpu_dedicated_memory_bytes', label: t("GPU 專用記憶體"), numeric: true, value: (p) => p.gpu_dedicated_memory_bytes, format: bytes },
      { id: 'gpu_shared_memory_bytes', label: t("GPU 共用記憶體"), numeric: true, value: (p) => p.gpu_shared_memory_bytes, format: bytes },
      { id: 'gpu_engine', label: t("GPU 引擎"), value: (p) => p.gpu_engine },
      { id: 'disk_read_bytes_per_sec', label: t("磁碟讀取 / 秒"), numeric: true, value: (p) => p.disk_read_bytes_per_sec, format: rate },
      { id: 'disk_written_bytes_per_sec', label: t("磁碟寫入 / 秒"), numeric: true, value: (p) => p.disk_written_bytes_per_sec, format: rate },
      { id: 'disk_read_bytes', label: t("累計磁碟讀取"), numeric: true, value: (p) => p.disk_read_bytes, format: bytes },
      { id: 'disk_written_bytes', label: t("累計磁碟寫入"), numeric: true, value: (p) => p.disk_written_bytes, format: bytes },
      { id: 'thread_count', label: t("執行緒數"), numeric: true, value: (p) => p.thread_count },
      { id: 'handle_count', label: t("控制代碼數"), numeric: true, value: (p) => p.handle_count },
      { id: 'priority', label: t("優先順序"), value: (p) => p.priority },
      { id: 'status', label: t("狀態"), value: (p) => p.status },
      { id: 'user_name', label: t("使用者"), value: (p) => p.user_name },
      { id: 'start_time', label: t("啟動時間"), numeric: true, value: (p) => p.start_time || null, format: (value) => new Date(Number(value) * 1000).toLocaleString() },
      { id: 'run_time_secs', label: t("執行時間"), numeric: true, value: (p) => p.run_time_secs, format: (value) => formatUptime(Number(value), t) },
      { id: 'command_line', label: t("命令列"), value: (p) => p.command_line || null },
      { id: 'working_directory', label: t("工作目錄"), value: (p) => p.working_directory || null },
      { id: 'exe_path', label: t("執行檔路徑"), value: (p) => p.exe_path || null },
    ];
  }, [snapshot?.total_memory_bytes, t]);
  const selected = columns.filter((column) => preferences.ids.includes(column.id));
  const visible = selected.length ? selected : columns.filter((column) => DEFAULT_COLUMNS.includes(column.id));
  const rows = useMemo(() => {
    const query = search.trim().toLowerCase();
    const rows = (snapshot?.processes ?? []).filter((p) =>
      !query || [p.name, p.pid, p.parent_pid, p.exe_path, p.command_line, p.user_name]
        .some((value) => value != null && String(value).toLowerCase().includes(query))
    );
    const column = columns.find((column) => column.id === sort.id);
    if (!column) return rows;
    return rows.sort((a, b) => {
      const left = column.value(a);
      const right = column.value(b);
      if (left == null) return right == null ? a.pid - b.pid : 1;
      if (right == null) return -1;
      const result = typeof left === 'number' && typeof right === 'number'
        ? left - right : String(left).localeCompare(String(right), undefined, { numeric: true });
      return (sort.descending ? -result : result) || a.pid - b.pid;
    });
  }, [snapshot, search, columns, sort]);

  const saveColumns = (ids: ColumnId[]) => {
    setPreferences({ ids, failed: false });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
      setSaveFailed(false);
    } catch (error) {
      console.warn('Could not save process column preferences:', error);
      setSaveFailed(true);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-3 text-xs">
        <span className="text-slate-400">{t("顯示 {shown} / {total} 個處理程序", { shown: rows.length, total: snapshot?.total_processes ?? 0 })}</span>
        <button type="button" aria-expanded={settingsOpen} aria-controls="process-column-settings"
          onClick={() => setSettingsOpen((open) => !open)}
          className="flex items-center gap-1.5 rounded-xl border border-slate-700 bg-slate-800 px-3.5 py-2 text-slate-200 hover:bg-slate-700">
          <Settings2 className="h-3.5 w-3.5" />{t("設定顯示欄位")}
        </button>
      </div>
      {(preferences.failed || saveFailed || selected.length === 0) && (
        <p role="alert" className="text-xs text-amber-300">{t("無法讀取或儲存欄位設定；目前選擇僅在此頁面生效。")}</p>
      )}
      {settingsOpen && (
        <fieldset id="process-column-settings" className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 text-xs">
          <legend className="px-2 font-semibold text-slate-200">{t("設定顯示欄位")}</legend>
          <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
            <p className="text-slate-400">{t("勾選要顯示的欄位，設定會自動保存。點擊表頭可排序。")}</p>
            <div className="flex gap-3">
              <button type="button" onClick={() => saveColumns(columns.map((column) => column.id))} className="text-cyan-300 hover:text-cyan-200">{t("顯示全部欄位")}</button>
              <button type="button" onClick={() => saveColumns(DEFAULT_COLUMNS)} className="text-slate-300 hover:text-white">{t("還原預設欄位")}</button>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-5">
            {columns.map((column) => (
              <label key={column.id} className="flex cursor-pointer items-center gap-2 text-slate-200">
                <input type="checkbox" checked={visible.some((item) => item.id === column.id)}
                  disabled={visible.length === 1 && visible[0].id === column.id}
                  onChange={(event) => saveColumns(columns.filter((item) => item.id === column.id
                    ? event.target.checked : visible.some((selected) => selected.id === item.id)).map((item) => item.id))}
                  className="accent-cyan-500" />
                {column.label}
              </label>
            ))}
          </div>
        </fieldset>
      )}
      {snapshot?.gpu_metrics_notice && (
        <p role="status" className="break-words rounded-xl border border-amber-500/20 bg-amber-500/10 p-3 text-xs text-amber-200">
          {t("部分 GPU 計數器無法取得。請確認 Windows WDDM 驅動與效能計數器可用。")}{' '}
          <span className="font-mono">{snapshot.gpu_metrics_notice}</span>
        </p>
      )}
      <p className="text-[11px] text-slate-500">{t("GPU % 為最忙碌引擎的使用率；GPU 記憶體為專用與共用記憶體總和。不可用表示權限不足、驅動不支援或尚無有效取樣。")}</p>
      <div className="overflow-x-auto rounded-2xl border border-slate-800 bg-slate-900/40">
        <table className="w-full text-left text-xs">
          <thead>
            <tr className="border-b border-slate-800 bg-slate-900 text-slate-400">
              {visible.map((column) => (
                <th key={column.id} aria-sort={sort.id === column.id ? sort.descending ? 'descending' : 'ascending' : 'none'}
                  className={`whitespace-nowrap px-3 py-2.5 ${column.numeric ? 'text-right' : ''}`}>
                  <button type="button" onClick={() => setSort({ id: column.id, descending: sort.id === column.id ? !sort.descending : Boolean(column.numeric) })}
                    className={`inline-flex items-center gap-1 hover:text-white ${column.numeric ? 'justify-end' : ''}`}>
                    {column.label}
                    {sort.id === column.id && (sort.descending ? <ArrowDown className="h-3 w-3" /> : <ArrowUp className="h-3 w-3" />)}
                  </button>
                </th>
              ))}
              <th className="sticky right-0 bg-slate-900 px-3 py-2.5 text-right">{t("操作")}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60">
            {rows.map((process) => (
              <tr key={process.pid} className="text-slate-200 hover:bg-slate-800/50">
                {visible.map((column) => {
                  const value = column.value(process);
                  const text = value == null ? t("不可用") : column.format ? column.format(value) : String(value);
                  return (
                    <td key={column.id} title={text}
                      className={`max-w-xs truncate whitespace-nowrap px-3 py-2 ${column.numeric ? 'text-right font-mono' : ''} ${column.id === 'pid' ? 'text-cyan-300' : value == null ? 'text-slate-500' : ''}`}>
                      {text}
                    </td>
                  );
                })}
                <td className="sticky right-0 bg-slate-950 px-3 py-2 text-right">
                  <button type="button" disabled={loading || process.pid === 0}
                    onClick={() => onKill(process.pid)}
                    className="whitespace-nowrap rounded bg-rose-500/20 px-2.5 py-1 text-rose-300 hover:bg-rose-500/40 disabled:opacity-40">
                    {t("終止行程")}
                  </button>
                </td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr><td colSpan={visible.length + 1} className="p-8 text-center text-slate-400">{loading ? t("Rust 引擎讀取中...") : t("沒有符合條件的處理程序")}</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

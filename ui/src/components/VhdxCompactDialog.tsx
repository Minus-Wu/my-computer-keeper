import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, CheckCircle2, Circle, RefreshCw, X, XCircle } from 'lucide-react';
import { formatBytes, keeperApi } from '../lib/bridge';
import type { CompactEnvironment, VhdxCompactStatus } from '../types/keeper';
import type { TranslateParams } from '../i18n';

interface Props {
  path: string;
  name: string;
  job: VhdxCompactStatus | null;
  t: (key: string, params?: TranslateParams) => string;
  tx: (message: string) => string;
  onJob: (job: VhdxCompactStatus | null) => void;
  onClose: () => void;
}

export function VhdxCompactDialog({ path, name, job, t, tx, onJob, onClose }: Props) {
  const [environment, setEnvironment] = useState<CompactEnvironment | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const [acknowledged, setAcknowledged] = useState(false);
  const [inspectRevision, setInspectRevision] = useState(0);
  const panel = useRef<HTMLDivElement>(null);
  const busy = submitting || uncertain || job?.state === 'running';
  const hasJob = job && job.state !== 'idle';
  const labels = {
    inspect: t("檢查環境與記錄原始狀態"),
    stop_docker: t("結束 Docker Desktop"),
    stop_wsl: t("結束 WSL"),
    compact: t("壓縮 VHDX"),
    start_wsl: t("啟動原本執行中的 WSL"),
    start_docker: t("啟動原本執行中的 Docker Desktop"),
  };
  const stateLabels = {
    pending: t("等待中"),
    running: t("進行中"),
    succeeded: t("完成"),
    skipped: t("略過"),
    failed: t("失敗"),
  };

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.focus();
    return () => previous?.focus();
  }, []);

  useEffect(() => {
    if (hasJob) return;
    let cancelled = false;
    setInspecting(true);
    setEnvironment(null);
    setError(null);
    keeperApi.inspectCompactEnvironment()
      .then((value) => { if (!cancelled) setEnvironment(value); })
      .catch((err: unknown) => { if (!cancelled) setError(String(err)); })
      .finally(() => { if (!cancelled) setInspecting(false); });
    return () => { cancelled = true; };
  }, [hasJob, inspectRevision]);

  useEffect(() => {
    if (!uncertain) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const reconcile = async () => {
      try {
        const status = await keeperApi.getCompactVhdxStatus();
        if (cancelled) return;
        if (status.path === path && (status.state === 'running' || status.recovery_required || hasJob)) onJob(status);
        setUncertain(false);
      } catch (err) {
        if (!cancelled) {
          setError(String(err));
          timer = setTimeout(reconcile, 1000);
        }
      }
    };
    timer = setTimeout(reconcile, 500);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [uncertain, onJob, path, hasJob]);

  const start = async (restoreOnly: boolean) => {
    if (submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      onJob(await (restoreOnly ? keeperApi.retryVhdxRestore() : keeperApi.compactVhdx(path)));
    } catch (err) {
      setError(String(err));
      // A lost response does not imply the backend did not admit the job.
      setUncertain(true);
    } finally {
      setSubmitting(false);
    }
  };
  const snapshot = hasJob ? job.environment : environment;
  const current = job?.steps.find((step) => step.state === 'running');

  return (
    <div className="fixed inset-0 z-[60] bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4">
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby="vhdx-dialog-title"
        aria-describedby="vhdx-dialog-description"
        tabIndex={-1}
        className="w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl border border-slate-700 bg-slate-900 p-6 shadow-2xl text-slate-200 space-y-5"
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            if (!busy) onClose();
          }
          if (event.key === 'Tab') {
            const elements = Array.from(panel.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), [tabindex="0"]') ?? []);
            const first = elements[0];
            const last = elements[elements.length - 1];
            if (!first) { event.preventDefault(); return; }
            if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.current)) {
              event.preventDefault(); last.focus();
            } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === panel.current)) {
              event.preventDefault(); first.focus();
            }
          }
        }}
      >
        <div className="flex justify-between items-start gap-4">
          <div className="min-w-0">
            <h2 id="vhdx-dialog-title" className="text-lg font-semibold text-white">{t("虛擬磁碟壓縮精靈")}</h2>
            <p className="mt-1 text-sm">{name}</p>
            <p className="text-xs text-slate-400 break-all font-mono">{path}</p>
          </div>
          <button type="button" disabled={busy} onClick={onClose} aria-label={t("關閉")} className="p-2 rounded-lg hover:bg-slate-800 disabled:opacity-40">
            <X className="w-5 h-5" />
          </button>
        </div>
        <p id="vhdx-dialog-description" className="text-sm text-amber-200 bg-amber-950/30 border border-amber-800/50 rounded-xl p-3">
          {t("請先儲存工作並備份重要資料。此流程會中斷所有 WSL 工作與 Docker 容器；完成或失敗後會復原原本執行中的環境，但不會還原終端工作與未設定自動重啟的容器。")}
          <span className="block mt-2">{t("壓縮時需要 UAC 授權。請勿關閉本程式或手動啟動 WSL／Docker；若有其他虛擬機使用此磁碟，也必須先停止。")}</span>
        </p>
        {inspecting && <p role="status" className="text-sm text-cyan-300">{t("正在檢查 Docker／WSL 環境...")}</p>}
        {snapshot && (
          <div className="text-sm space-y-1">
            <p>{t("Docker Desktop：{state}", { state: snapshot.docker_running ? t("執行中，將停止並重新啟動") : t("未執行，將略過") })}</p>
            <p className="break-all">{t("原本執行中的 WSL：{names}", { names: snapshot.wsl_distributions.join(', ') || t("無") })}</p>
          </div>
        )}
        <ol className="space-y-3">
          {Object.entries(labels).map(([id, label], index) => {
            const step = hasJob ? job.steps.find((item) => item.id === id) : null;
            const state = step?.state ?? 'pending';
            return (
              <li key={id} className="rounded-xl bg-slate-950/50 p-3 border border-slate-800">
                <div className="flex items-center gap-3 text-sm">
                  {state === 'running' ? <RefreshCw className="w-4 h-4 animate-spin text-cyan-300" /> :
                    state === 'succeeded' ? <CheckCircle2 className="w-4 h-4 text-emerald-300" /> :
                      state === 'failed' ? <XCircle className="w-4 h-4 text-rose-300" /> : <Circle className="w-4 h-4 text-slate-500" />}
                  <span className="flex-1">{index + 1}. {label}</span>
                  <span className="text-xs text-slate-400">{stateLabels[state]}</span>
                </div>
                {step?.message && <p className="mt-2 text-xs text-rose-300 whitespace-pre-wrap break-all">{tx(step.message)}</p>}
              </li>
            );
          })}
        </ol>
        {busy && (
          <div role="status" aria-live="polite" className="space-y-2 text-sm text-cyan-200">
            <p>{current ? labels[current.id] : t("正在啟動流程...")}</p>
            {current?.id === 'compact' && (
              job?.progress_percent != null ? (
                <div>
                  <progress className="w-full accent-cyan-400" max={100} value={job.progress_percent} aria-label={t("壓縮進度")} />
                  <p className="text-right font-mono">{job.progress_percent}%</p>
                </div>
              ) : <p>{t("等待 UAC 授權或壓縮進度回報；無百分比時仍會持續顯示步驟狀態。")}</p>
            )}
          </div>
        )}
        {hasJob && !busy && (
          <div role="status" className={`rounded-xl p-3 text-sm ${job.state === 'succeeded' ? 'bg-emerald-950/40 text-emerald-200' : 'bg-rose-950/40 text-rose-200'}`}>
            <p>{job.state === 'succeeded' ? t("壓縮與環境復原已完成") : job.compact_completed ? t("磁碟已壓縮，但部分步驟未完成") : t("壓縮未完成，請查看各步驟的錯誤")}</p>
            {job.compact_completed && job.size_after_bytes != null && (
              <p className="mt-1">{t("虛擬磁碟壓縮完成：{before} → {after}（釋放 {freed}）", {
                before: formatBytes(job.size_before_bytes), after: formatBytes(job.size_after_bytes),
                freed: formatBytes(Math.max(0, job.size_before_bytes - job.size_after_bytes)),
              })}</p>
            )}
            {job.recovery_required && <p className="mt-2">{t("環境尚未完全復原。可在此重試啟動，不會再次壓縮磁碟。")}</p>}
          </div>
        )}
        {error && <p role="alert" className="text-sm text-rose-300 whitespace-pre-wrap break-all">{tx(error)}</p>}
        {uncertain && <p role="alert" className="text-sm text-amber-200">{t("無法取得壓縮狀態，將自動重試；請勿關閉程式或啟動 WSL／Docker。")}</p>}
        {!hasJob && (
          <label className="flex items-start gap-2 text-sm">
            <input type="checkbox" checked={acknowledged} onChange={(event) => setAcknowledged(event.target.checked)} disabled={busy} className="mt-1" />
            {t("我已儲存工作，並同意停止 Docker Desktop 與所有 WSL 發行版")}
          </label>
        )}
        <div className="flex flex-wrap justify-end gap-3">
          {!hasJob && error && <button type="button" disabled={busy || inspecting} onClick={() => setInspectRevision((value) => value + 1)} className="px-4 py-2 rounded-lg bg-slate-800 disabled:opacity-40">{t("重新檢查")}</button>}
          {hasJob && job.recovery_required && <button type="button" disabled={busy} onClick={() => void start(true)} className="px-4 py-2 rounded-lg bg-amber-800 disabled:opacity-40">{t("重試環境復原")}</button>}
          {hasJob && job.state === 'failed' && !job.recovery_required && <button type="button" disabled={busy} onClick={() => { onJob(null); setAcknowledged(false); }} className="px-4 py-2 rounded-lg bg-slate-800">{t("重新壓縮")}</button>}
          {!hasJob && <button type="button" disabled={busy || inspecting || !environment || !acknowledged} onClick={() => void start(false)} className="px-4 py-2 rounded-lg bg-cyan-700 disabled:opacity-40">{t("開始壓縮流程")}</button>}
          <button type="button" disabled={busy} onClick={onClose} className="px-4 py-2 rounded-lg bg-slate-800 disabled:opacity-40">{hasJob ? t("關閉") : t("取消")}</button>
        </div>
        {busy && <p className="text-xs text-slate-400 flex gap-2"><AlertTriangle className="w-4 h-4 shrink-0" />{t("流程進行中無法關閉精靈，請等待壓縮與復原完成。")}</p>}
      </div>
    </div>
  );
}

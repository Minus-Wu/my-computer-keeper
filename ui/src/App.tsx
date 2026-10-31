import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ProcessTable } from './components/ProcessTable';
import { VhdxCompactDialog } from './components/VhdxCompactDialog';
import {
  availableLocales,
  detectLocale,
  persistLocale,
  translate,
  translateBackend,
  type TranslateParams,
} from './i18n';
import {
  Activity,
  AlertTriangle,
  Archive,
  ArrowLeftRight,
  ArrowUp,
  Binary,
  BookOpen,
  Check,
  CheckCircle2,
  Copy,
  Cpu,
  Database,
  Disc,
  ExternalLink,
  Eye,
  FileCode,
  FileText,
  Film,
  Folder,
  FolderSync,
  HardDrive,
  Image as ImageIcon,
  Layers,
  Lock,
  Minimize2,
  Music,
  Network,
  PieChart,
  Power,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Trash2,
  Unlock,
  Wrench,
  X,
  ZoomIn,
  ZoomOut,
  Gauge,
  XCircle,
} from 'lucide-react';
import {
  computeParentPath,
  formatBytes,
  formatTimestamp,
  formatUptime,
  keeperApi,
  normalizePathKey,
} from './lib/bridge';
import type {
  CompareMode,
  DirectoryListing,
  DriveInfo,
  DuplicateScanReport,
  FileEntry,
  GeneralItemPreview,
  ImagePreviewResult,
  JunkScanReport,
  LockHunterReport,
  PortBindingEntry,
  SafeEjectReport,
  SyncComparisonReport,
  SyncExecutionSummary,
  SyncStrategy,
  SystemMaintenanceOverview,
  SystemSnapshot,
  TreeSizeProgress,
  TreeSizeReport,
  VhdxCompactStatus,
  VolumeCapabilities,
} from './types/keeper';

type ModuleTab =
  | 'explorer'
  | 'treesize'
  | 'taskmgr'
  | 'sync'
  | 'ops';

const TREEMAP_COLORS = [
  'bg-cyan-500/80 border-cyan-300/40',
  'bg-indigo-500/80 border-indigo-300/40',
  'bg-emerald-500/80 border-emerald-300/40',
  'bg-amber-500/80 border-amber-300/40',
  'bg-rose-500/80 border-rose-300/40',
  'bg-violet-500/80 border-violet-300/40',
  'bg-sky-500/80 border-sky-300/40',
  'bg-teal-500/80 border-teal-300/40',
];

export function App() {
  const [locale, setLocale] = useState<string>(detectLocale);
  const t = useCallback(
    (key: string, params?: TranslateParams) => translate(locale, key, params),
    [locale]
  );
  const tx = useCallback(
    (text: string | null | undefined) => translateBackend(locale, text),
    [locale]
  );
  useEffect(() => {
    document.documentElement.lang = locale;
    persistLocale(locale);
  }, [locale]);

  // 桌面應用生命週期：心跳與視窗關閉通知後端結束背景行程
  useEffect(() => {
    const heartbeatTimer = setInterval(() => {
      fetch('/api/heartbeat', { method: 'POST' }).catch(() => {});
    }, 4000);

    const handleUnload = () => {
      try {
        if (navigator.sendBeacon) {
          navigator.sendBeacon('/api/shutdown');
        } else {
          fetch('/api/shutdown', { method: 'POST', keepalive: true }).catch(() => {});
        }
      } catch {
        // ignore
      }
    };

    window.addEventListener('beforeunload', handleUnload);
    window.addEventListener('pagehide', handleUnload);

    return () => {
      clearInterval(heartbeatTimer);
      window.removeEventListener('beforeunload', handleUnload);
      window.removeEventListener('pagehide', handleUnload);
    };
  }, []);
  const [activeTab, setActiveTab] = useState<ModuleTab>('explorer');
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [currentPath, setCurrentPath] = useState<string>('');
  const [pathInput, setPathInput] = useState<string>('');
  const [statusBanner, setStatusBanner] = useState<string | null>(null);
  const [errorBanner, setErrorBanner] = useState<string | null>(null);
  const [loading, setLoading] = useState<boolean>(false);

  // 網路磁碟機能力檢測器狀態
  const [probeUncInput, setProbeUncInput] = useState<string>(
    '\\\\192.168.1.200\\NAS-Media\\Photos'
  );
  const [probedCaps, setProbedCaps] = useState<VolumeCapabilities | null>(null);

  // 1. 檔案總管狀態
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [searchFilter, setSearchFilter] = useState<string>('');
  const [selectedEntry, setSelectedEntry] = useState<FileEntry | null>(null);
  const [generalPreview, setGeneralPreview] =
    useState<GeneralItemPreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState<boolean>(false);
  const [imagePreview, setImagePreview] = useState<ImagePreviewResult | null>(
    null
  );
  const [imageZoom, setImageZoom] = useState<number>(100);
  const [copiedText, setCopiedText] = useState<boolean>(false);
  const [textViewMode, setTextViewMode] = useState<'raw' | 'parsed'>('raw');
  const [confirmNetworkPermanentDelete, setConfirmNetworkPermanentDelete] =
    useState<boolean>(false);

  // 檔案總管內嵌 LockHunter Modal 狀態
  const [lockModalOpen, setLockModalOpen] = useState<boolean>(false);
  const [lockModalPath, setLockModalPath] = useState<string>('');
  const [lockModalLoading, setLockModalLoading] = useState<boolean>(false);
  const [lockModalReport, setLockModalReport] =
    useState<LockHunterReport | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && lockModalOpen) {
        setLockModalOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [lockModalOpen]);



  // 2. TreeSize 狀態
  const [treeSizePath, setTreeSizePath] = useState<string>('');
  const [treeSizeReport, setTreeSizeReport] = useState<TreeSizeReport | null>(
    null
  );
  const treeSizeCacheRef = useRef<Map<string, TreeSizeReport>>(new Map());
  const treeSizeParentPath = useMemo(() => {
    return treeSizeReport?.parent_path ?? computeParentPath(treeSizePath);
  }, [treeSizeReport?.parent_path, treeSizePath]);
  const [treeSizeLoading, setTreeSizeLoading] = useState<boolean>(false);
  const [treeSizeScanTarget, setTreeSizeScanTarget] = useState<string>('');
  const [scanElapsedSecs, setScanElapsedSecs] = useState<number>(0);
  const [treeSizeProgress, setTreeSizeProgress] = useState<TreeSizeProgress | null>(null);
  const [treeSizeScanRate, setTreeSizeScanRate] = useState<number>(0);
  const activeTreeSizeScanIdRef = useRef<string | null>(null);
  const treeSizePollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const lastProgressSampleRef = useRef<{ time: number; files: number }>({ time: 0, files: 0 });

  const stopTreeSizePolling = useCallback(() => {
    if (treeSizePollTimerRef.current) {
      clearInterval(treeSizePollTimerRef.current);
      treeSizePollTimerRef.current = null;
    }
  }, []);

  const handleCancelTreeSize = useCallback(async () => {
    const activeScanId = activeTreeSizeScanIdRef.current;
    stopTreeSizePolling();
    if (activeScanId) {
      try {
        await keeperApi.cancelTreeSizeScan(activeScanId);
      } catch {
        // 略過取消錯誤
      }
      activeTreeSizeScanIdRef.current = null;
    }
    setTreeSizeLoading(false);
    setLoading(false);
    notifyStatus(t("已取消磁碟空間分析"));
  }, [stopTreeSizePolling]);

  useEffect(() => {
    return () => {
      stopTreeSizePolling();
    };
  }, [stopTreeSizePolling]);
  const [vhdxJob, setVhdxJob] = useState<VhdxCompactStatus | null>(null);
  const [vhdxTarget, setVhdxTarget] = useState<{ path: string; name: string } | null>(null);
  const [vhdxPollError, setVhdxPollError] = useState<string | null>(null);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;
    if (treeSizeLoading) {
      setScanElapsedSecs(0);
      const start = Date.now();
      timer = setInterval(() => {
        setScanElapsedSecs(Number(((Date.now() - start) / 1000).toFixed(1)));
      }, 100);
    } else {
      setScanElapsedSecs(0);
    }
    return () => {
      if (timer) clearInterval(timer);
    };
  }, [treeSizeLoading]);

  // 3. 工作管理員 + Port Hunter 狀態
  const [sysSnapshot, setSysSnapshot] = useState<SystemSnapshot | null>(null);
  const [ports, setPorts] = useState<PortBindingEntry[]>([]);
  const [taskSubTab, setTaskSubTab] = useState<'processes' | 'ports'>(
    'processes'
  );
  const [taskSearch, setTaskSearch] = useState<string>('');

  // 4. FreeFileSync 狀態
  const [syncLeftPath, setSyncLeftPath] = useState<string>('');
  const [syncRightPath, setSyncRightPath] = useState<string>('');
  const [syncDriveSide, setSyncDriveSide] = useState<'left' | 'right'>('left');
  const [syncCompareMode, setSyncCompareMode] =
    useState<CompareMode>('time_and_size');
  const [syncStrategy, setSyncStrategy] = useState<SyncStrategy>('mirror');
  const [syncReport, setSyncReport] = useState<SyncComparisonReport | null>(
    null
  );
  const [syncExecResult, setSyncExecResult] =
    useState<SyncExecutionSummary | null>(null);

  // 5. 電腦本體維運中心 (Ops) 狀態
  const [opsTargetPath, setOpsTargetPath] = useState<string>('');
  const [maintOverview, setMaintOverview] =
    useState<SystemMaintenanceOverview | null>(null);
  const [junkReport, setJunkReport] = useState<JunkScanReport | null>(null);
  const [dupReport, setDupReport] = useState<DuplicateScanReport | null>(null);

  const notifyStatus = (msg: string) => {
    setErrorBanner(null);
    setStatusBanner(msg);
  };

  const notifyError = (err: unknown) => {
    const message = err instanceof Error ? err.message : String(err);
    setErrorBanner(message);
  };

  // 載入磁碟機清單與初始目錄
  const loadDirectory = useCallback(async (targetPath?: string) => {
    setLoading(true);
    setErrorBanner(null);
    try {
      const data = await keeperApi.listDirectory(targetPath);
      setListing(data);
      setCurrentPath(data.current_path);
      setPathInput(data.current_path);
      setTreeSizePath((prev) => prev || data.current_path);
      setSyncLeftPath((prev) => prev || data.current_path);
      setSyncRightPath((prev) => prev || data.current_path);
      setOpsTargetPath((prev) => prev || data.current_path);
      setSelectedEntry(null);
      keeperApi
        .previewEntry(data.current_path)
        .then((p) => {
          setGeneralPreview(p);
          setImagePreview(null);
        })
        .catch(() => setGeneralPreview(null));
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    keeperApi
      .getDrives()
      .then(setDrives)
      .catch(() => {});
    keeperApi
      .getCompactVhdxStatus()
      .then((job) => {
        setVhdxJob(job);
        if (job.state === 'running' || job.recovery_required) {
          setVhdxTarget({ path: job.path, name: job.path.split(/[\\/]/).pop() ?? job.path });
        }
      })
      .catch(notifyError);
    loadDirectory();
  }, [loadDirectory]);

  // 當選擇檔案或資料夾時，自動載入全方位預覽（資料夾說明、圖片、純文字、PDF、ZIP、影音、Hex 等）
  const handleSelectEntry = async (entry: FileEntry, forceNetwork = false) => {
    setSelectedEntry(entry);
    setCopiedText(false);
    setTextViewMode('raw');
    setPreviewLoading(true);
    try {
      const preview = await keeperApi.previewEntry(entry.path, forceNetwork);
      setGeneralPreview(preview);
      setImagePreview(preview.image_preview);
      setImageZoom(100);
    } catch (err) {
      notifyError(err);
      setGeneralPreview(null);
      setImagePreview(null);
    } finally {
      setPreviewLoading(false);
    }
  };


  const handleDeleteEntry = async (entry: FileEntry) => {
    try {
      const msg = await keeperApi.deleteEntry(
        entry.path,
        true,
        confirmNetworkPermanentDelete
      );
      notifyStatus(msg);
      setSelectedEntry(null);
      setImagePreview(null);
      setGeneralPreview(null);
      await loadDirectory(currentPath);
    } catch (err) {
      notifyError(err);
    }
  };

  // TreeSize 分析（支援非同步即時進度與快取防重掃描）
  const runTreeSize = useCallback(
    async (overridePath?: string, forceRefresh = false) => {
      const target = overridePath ?? treeSizePath ?? currentPath;
      if (!target) return;
      const key = normalizePathKey(target);
      if (!forceRefresh && treeSizeCacheRef.current.has(key)) {
        const cached = treeSizeCacheRef.current.get(key)!;
        setTreeSizeReport(cached);
        setTreeSizePath(cached.root_path);
        return;
      }
      if (activeTreeSizeScanIdRef.current) {
        stopTreeSizePolling();
        keeperApi.cancelTreeSizeScan(activeTreeSizeScanIdRef.current).catch(() => {});
        activeTreeSizeScanIdRef.current = null;
      }
      setTreeSizeScanTarget(target);
      setTreeSizeProgress(null);
      setTreeSizeScanRate(0);
      lastProgressSampleRef.current = { time: Date.now(), files: 0 };
      setTreeSizeLoading(true);
      setLoading(true);
      setErrorBanner(null);
      try {
        const { scan_id } = await keeperApi.startTreeSizeScan(target);
        activeTreeSizeScanIdRef.current = scan_id;
        const pollTimer = setInterval(async () => {
          if (activeTreeSizeScanIdRef.current !== scan_id) {
            clearInterval(pollTimer);
            return;
          }
          try {
            const state = await keeperApi.getTreeSizeProgress(scan_id);
            if (!state) return;
            const now = Date.now();
            const elapsedSinceSample = (now - lastProgressSampleRef.current.time) / 1000;
            if (elapsedSinceSample >= 0.4) {
              const filesDiff = state.progress.processed_files - lastProgressSampleRef.current.files;
              const rate = Math.max(0, Math.round(filesDiff / elapsedSinceSample));
              setTreeSizeScanRate(rate);
              lastProgressSampleRef.current = { time: now, files: state.progress.processed_files };
            }
            setTreeSizeProgress(state.progress);
            if (state.status === 'completed' && state.report) {
              clearInterval(pollTimer);
              treeSizePollTimerRef.current = null;
              activeTreeSizeScanIdRef.current = null;
              const report = state.report;
              const reportKey = normalizePathKey(report.root_path);
              treeSizeCacheRef.current.set(reportKey, report);
              treeSizeCacheRef.current.set(key, report);
              setTreeSizeReport(report);
              setTreeSizePath(report.root_path);
              setTreeSizeLoading(false);
              setLoading(false);
            } else if (state.status === 'cancelled') {
              clearInterval(pollTimer);
              treeSizePollTimerRef.current = null;
              activeTreeSizeScanIdRef.current = null;
              setTreeSizeLoading(false);
              setLoading(false);
            } else if (state.status === 'failed') {
              clearInterval(pollTimer);
              treeSizePollTimerRef.current = null;
              activeTreeSizeScanIdRef.current = null;
              setTreeSizeLoading(false);
              setLoading(false);
              notifyError(state.error ?? t("空間分析失敗"));
            }
          } catch {
            // 輪詢網路波動不中斷
          }
        }, 160);
        treeSizePollTimerRef.current = pollTimer;
      } catch (err) {
        setTreeSizeLoading(false);
        setLoading(false);
        notifyError(err);
      }
    },
    [treeSizePath, currentPath, stopTreeSizePolling]
  );

  // 切換上一層目錄：若有快取則瞬間載入，未掃描則切換路徑提示手動分析（不自動觸發重掃）
  const handleTreeSizeGoParent = useCallback(() => {
    if (!treeSizeParentPath || treeSizeLoading) return;
    const parentKey = normalizePathKey(treeSizeParentPath);
    setTreeSizePath(treeSizeParentPath);
    if (treeSizeCacheRef.current.has(parentKey)) {
      setTreeSizeReport(treeSizeCacheRef.current.get(parentKey)!);
    } else {
      setTreeSizeReport(null);
    }
  }, [treeSizeParentPath, treeSizeLoading]);

  // Treemap 點選子目錄下鑽：快取命中則瞬間呈現，否則單獨掃描該子目錄
  const handleTreeSizeDrillDown = useCallback(
    (childPath: string) => {
      if (treeSizeLoading) return;
      const key = normalizePathKey(childPath);
      setTreeSizePath(childPath);
      if (treeSizeCacheRef.current.has(key)) {
        setTreeSizeReport(treeSizeCacheRef.current.get(key)!);
      } else {
        runTreeSize(childPath, false);
      }
    },
    [treeSizeLoading, runTreeSize]
  );

  const handleCompactVhdx = (path: string, name: string) => {
    if (vhdxJob?.state === 'running' || vhdxJob?.recovery_required) {
      setVhdxTarget({ path: vhdxJob.path, name: vhdxJob.path.split(/[\\/]/).pop() ?? vhdxJob.path });
      return;
    }
    setVhdxJob(null);
    setVhdxTarget({ path, name });
  };

  // 壓縮作業於後端背景執行，進行中時輪詢狀態直到完成
  useEffect(() => {
    if (vhdxJob?.state !== 'running') return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const status = await keeperApi.getCompactVhdxStatus();
        if (cancelled) return;
        setVhdxPollError(null);
        setVhdxJob(status);
        if (status.state === 'succeeded') {
          const after = status.size_after_bytes ?? status.size_before_bytes;
          notifyStatus(
            t("虛擬磁碟壓縮完成：{before} → {after}（釋放 {freed}）", {
              before: formatBytes(status.size_before_bytes),
              after: formatBytes(after),
              freed: formatBytes(Math.max(0, status.size_before_bytes - after)),
            })
          );
        } else if (status.state === 'failed') {
          notifyError(status.message);
        }
      } catch (err) {
        if (!cancelled) setVhdxPollError(String(err));
      }
      if (!cancelled) timer = setTimeout(poll, 1000);
    };
    timer = setTimeout(poll, 500);
    return () => { cancelled = true; clearTimeout(timer); };
  }, [vhdxJob?.state, t]);

  useEffect(() => {
    if (vhdxJob?.state !== 'running') return;
    const preventUnload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = ''; };
    window.addEventListener('beforeunload', preventUnload);
    return () => window.removeEventListener('beforeunload', preventUnload);
  }, [vhdxJob?.state]);

  // 工作管理員與 Port 監控
  const refreshTaskManager = useCallback(async () => {
    setLoading(true);
    setErrorBanner(null);
    try {
      const [snap, portList] = await Promise.all([
        keeperApi.getSystemSnapshot(500),
        keeperApi.listPorts(),
      ]);
      setSysSnapshot(snap);
      setPorts(portList);
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  }, []);

  const handleKillProcess = async (pid: number) => {
    try {
      const msg = await keeperApi.killProcess(pid);
      notifyStatus(msg);
      await refreshTaskManager();
    } catch (err) {
      notifyError(err);
    }
  };

  // 檔案總管就地 LockHunter Modal 處理函式
  const openLockModal = async (targetPath: string) => {
    setLockModalPath(targetPath);
    setLockModalOpen(true);
    setLockModalLoading(true);
    setLockModalReport(null);
    try {
      const report = await keeperApi.inspectLocks(targetPath);
      setLockModalReport(report);
    } catch (err) {
      notifyError(err);
    } finally {
      setLockModalLoading(false);
    }
  };

  const refreshLockModal = async () => {
    if (!lockModalPath) return;
    setLockModalLoading(true);
    try {
      const report = await keeperApi.inspectLocks(lockModalPath);
      setLockModalReport(report);
    } catch (err) {
      notifyError(err);
    } finally {
      setLockModalLoading(false);
    }
  };

  const handleKillLockProcess = async (pid: number) => {
    try {
      const msg = await keeperApi.killProcess(pid);
      notifyStatus(msg);
      await refreshLockModal();
    } catch (err) {
      notifyError(err);
    }
  };

  const isDriveTarget = (p?: string | null) => {
    if (!p) return false;
    const clean = p.replace(/^\\\\?\\/, '').trim();
    return /^[a-zA-Z]:[\\/]?$/.test(clean);
  };

  const [ejectingDrive, setEjectingDrive] = useState<string | null>(null);
  const [safeEjectModalReport, setSafeEjectModalReport] =
    useState<SafeEjectReport | null>(null);

  const handleSafeEjectDrive = async (drivePath: string) => {
    try {
      setEjectingDrive(drivePath);
      const rep = await keeperApi.ejectDrive(drivePath);
      setSafeEjectModalReport(rep);
      notifyStatus(rep.message);
      if (lockModalOpen) {
        await refreshLockModal();
      }
      await keeperApi.getDrives().then(setDrives).catch(() => {});
    } catch (err) {
      notifyError(err);
    } finally {
      setEjectingDrive(null);
    }
  };

  // FreeFileSync 比對與執行
  const runSyncCompare = async () => {
    if (!syncLeftPath || !syncRightPath) return;
    setLoading(true);
    setErrorBanner(null);
    setSyncExecResult(null);
    try {
      const report = await keeperApi.compareSync(
        syncLeftPath,
        syncRightPath,
        syncCompareMode,
        syncStrategy
      );
      setSyncReport(report);
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  };

  const runSyncExecute = async (dryRun: boolean) => {
    if (!syncLeftPath || !syncRightPath) return;
    setLoading(true);
    setErrorBanner(null);
    try {
      const summary = await keeperApi.executeSync(
        syncLeftPath,
        syncRightPath,
        syncCompareMode,
        syncStrategy,
        dryRun
      );
      setSyncExecResult(summary);
      notifyStatus(
        dryRun
          ? t("已完成 FreeFileSync 同步預演 (Dry-Run)，未實際修改磁碟檔案。")
          : t("已完成 FreeFileSync 檔案同步作業！")
      );
      await runSyncCompare();
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  };

  // 系統維運中心
  const loadSystemOps = useCallback(async () => {
    setLoading(true);
    setErrorBanner(null);
    try {
      const overview = await keeperApi.inspectMaintenance();
      setMaintOverview(overview);
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  }, []);

  const runJunkScan = async () => {
    if (!opsTargetPath) return;
    setLoading(true);
    setErrorBanner(null);
    try {
      const rep = await keeperApi.scanJunk(opsTargetPath);
      setJunkReport(rep);
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  };

  const runDuplicateScan = async () => {
    if (!opsTargetPath) return;
    setLoading(true);
    setErrorBanner(null);
    try {
      const rep = await keeperApi.findDuplicates(opsTargetPath);
      setDupReport(rep);
    } catch (err) {
      notifyError(err);
    } finally {
      setLoading(false);
    }
  };

  const runProbeCapabilities = async (target: string) => {
    try {
      const caps = await keeperApi.inspectCapabilities(target);
      setProbedCaps(caps);
    } catch (err) {
      notifyError(err);
    }
  };

  // 切換模組時自動載入對應資料（TreeSize 僅同步路徑或快取，絕不自動發起重度磁碟掃描）
  useEffect(() => {
    if (activeTab === 'treesize') {
      if (!treeSizePath && currentPath) {
        setTreeSizePath(currentPath);
      }
      const targetPath = treeSizePath || currentPath;
      if (targetPath) {
        const key = normalizePathKey(targetPath);
        if (!treeSizeReport && treeSizeCacheRef.current.has(key)) {
          setTreeSizeReport(treeSizeCacheRef.current.get(key)!);
        }
      }
    } else if (activeTab === 'taskmgr' && !sysSnapshot) {
      refreshTaskManager();
    } else if (activeTab === 'ops' && !maintOverview) {
      loadSystemOps();
    }
  }, [
    activeTab,
    currentPath,
    treeSizePath,
    treeSizeReport,
    sysSnapshot,
    maintOverview,
    refreshTaskManager,
    loadSystemOps,
  ]);

  const filteredEntries = useMemo(() => {
    if (!listing) return [];
    return listing.entries.filter((item) => {
      if (
        searchFilter.trim() &&
        !item.name.toLowerCase().includes(searchFilter.trim().toLowerCase())
      ) {
        return false;
      }
      return true;
    });
  }, [listing, searchFilter]);

  const updateTreeSizePath = (path: string) => {
    setTreeSizePath(path);
    setTreeSizeReport(treeSizeCacheRef.current.get(normalizePathKey(path)) ?? null);
  };

  const updateSyncPath = (path: string, side: 'left' | 'right') => {
    if (side === 'left') setSyncLeftPath(path);
    else setSyncRightPath(path);
    setSyncReport(null);
    setSyncExecResult(null);
  };

  const updateOpsPath = (path: string) => {
    setOpsTargetPath(path);
    setJunkReport(null);
    setDupReport(null);
  };

  const handleSelectDrive = (path: string) => {
    switch (activeTab) {
      case 'explorer':
        loadDirectory(path);
        break;
      case 'treesize':
        updateTreeSizePath(path);
        break;
      case 'sync':
        updateSyncPath(path, syncDriveSide);
        break;
      case 'ops':
        updateOpsPath(path);
        break;
    }
  };

  const storagePath = activeTab === 'treesize' ? treeSizePath
    : activeTab === 'sync' ? (syncDriveSide === 'left' ? syncLeftPath : syncRightPath)
    : activeTab === 'ops' ? opsTargetPath : currentPath;
  const capabilityPaths = useMemo(() => {
    const paths = activeTab === 'sync' ? [syncLeftPath, syncRightPath]
      : activeTab === 'taskmgr' || activeTab === 'explorer' ? [] : [storagePath];
    return [...new Set(paths.filter(Boolean))];
  }, [activeTab, storagePath, syncLeftPath, syncRightPath]);
  const [directoryCapabilities, setDirectoryCapabilities] = useState<{
    paths: string[];
    values: VolumeCapabilities[];
  }>({ paths: [], values: [] });

  useEffect(() => {
    if (!capabilityPaths.length) return;
    let cancelled = false;
    Promise.all(capabilityPaths.map((path) => keeperApi.inspectCapabilities(path)))
      .then((values) => {
        if (!cancelled) setDirectoryCapabilities({ paths: capabilityPaths, values });
      })
      .catch((err) => {
        if (!cancelled) notifyError(err);
      });
    return () => { cancelled = true; };
  }, [capabilityPaths]);

  const activeCapabilities = activeTab === 'explorer'
    ? listing ? [listing.capabilities] : []
    : directoryCapabilities.paths === capabilityPaths ? directoryCapabilities.values : [];

  // 右方預覽區域焦點目標（選中的項目，若無則為當前瀏覽的目錄）
  const previewTarget = useMemo(() => {
    if (selectedEntry) {
      return {
        path: selectedEntry.path,
        name: selectedEntry.name,
        isDir: selectedEntry.is_dir,
        isEntry: true,
      };
    }
    const curPath = generalPreview?.dir_preview?.path || currentPath;
    const curName =
      generalPreview?.dir_preview?.name || listing?.current_path || '';
    return {
      path: curPath,
      name: curName,
      isDir: true,
      isEntry: false,
    };
  }, [selectedEntry, generalPreview, currentPath, listing]);

  return (
    <div className="h-screen overflow-hidden flex flex-col bg-slate-950 text-slate-100">
      {/* 頂部系統標題與模組導覽列 */}
      <header className="border-b border-slate-800 bg-slate-900/90 backdrop-blur px-4 py-2 flex items-center justify-between gap-3 shrink-0">
        <div className="flex items-center gap-2">
          {/* 核心模組導覽 Tabs 與應用程式圖示 */}
          <nav className="flex flex-wrap items-center gap-1.5 bg-slate-950/80 p-1 rounded-xl border border-slate-800">
            <div
              title="My Computer Keeper"
              className="w-8 h-8 rounded-lg bg-gradient-to-br from-cyan-500 to-blue-600 flex items-center justify-center shadow-md shadow-cyan-500/20 text-white shrink-0 cursor-default mx-1 select-none"
            >
              <HardDrive className="w-4 h-4" />
            </div>

            {[
              { id: 'explorer', label: t("檔案總管"), icon: Folder },
              { id: 'treesize', label: t("TreeSize 空間分析"), icon: PieChart },
              { id: 'taskmgr', label: t("工作管理員 & Port"), icon: Activity },
              { id: 'sync', label: t("FreeFileSync 同步"), icon: FolderSync },
              { id: 'ops', label: t("電腦本體維運中心"), icon: Wrench },
            ].map((tab) => {
              const Icon = tab.icon;
              const isActive = activeTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id as ModuleTab)}
                  className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-all ${
                    isActive
                      ? 'bg-cyan-500 text-slate-950 shadow-md shadow-cyan-500/20 font-semibold'
                      : 'text-slate-300 hover:text-white hover:bg-slate-800/70'
                  }`}
                >
                  <Icon className="w-4 h-4" />
                  {tab.label}
                </button>
              );
            })}
          </nav>
        </div>

        <div className="flex items-center gap-2">
          <select
            value={locale}
            onChange={(e) => setLocale(e.target.value)}
            aria-label="Language"
            title="Language"
            className="text-xs bg-slate-950/80 border border-slate-800 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:border-cyan-500 cursor-pointer"
          >
            {availableLocales.map((l) => (
              <option key={l.code} value={l.code}>
                {l.name}
              </option>
            ))}
          </select>

          <button
            disabled={vhdxJob?.state === 'running'}
            onClick={() => {
              if (window.confirm(t("確定要結束並關閉程式嗎？"))) {
                try {
                  if (navigator.sendBeacon) {
                    navigator.sendBeacon('/api/shutdown');
                  } else {
                    fetch('/api/shutdown', { method: 'POST' }).catch(() => {});
                  }
                } catch {
                  // ignore
                }
                window.close();
              }
            }}
            title={t("關閉並結束程式")}
            className="text-xs bg-rose-950/40 hover:bg-rose-900/60 border border-rose-800/60 text-rose-300 hover:text-rose-100 rounded-lg px-2.5 py-1.5 flex items-center gap-1.5 transition-colors cursor-pointer"
          >
            <Power className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">{t("結束程式")}</span>
          </button>
        </div>
      </header>

      {/* 全域狀態與錯誤提示列 */}
      {(statusBanner || errorBanner) && (
        <div
          className={`px-5 py-2.5 text-xs flex items-center justify-between border-b ${
            errorBanner
              ? 'bg-rose-950/80 border-rose-800 text-rose-200'
              : 'bg-emerald-950/80 border-emerald-800 text-emerald-200'
          }`}
        >
          <div className="flex items-center gap-2">
            {errorBanner ? (
              <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0" />
            ) : (
              <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
            )}
            <span>{tx(errorBanner ?? statusBanner)}</span>
          </div>
          <button
            onClick={() => {
              setErrorBanner(null);
              setStatusBanner(null);
            }}
            className="text-slate-400 hover:text-white px-2"
          >
            ✕
          </button>
        </div>
      )}

      {/* 網路磁碟機「有限功能」安全降級橫幅（當目前路徑位於網路磁碟或啟用檢測時顯示） */}
      {activeCapabilities.map((caps, index) => caps.is_network_drive && (
        <div key={index} className="bg-amber-950/60 border-b border-amber-700/60 px-5 py-2.5 text-xs text-amber-200 flex flex-col gap-1">
          <div className="flex items-center gap-2 font-semibold text-amber-300">
            <Network className="w-4 h-4" />
            <span>
              {t("🌐 網路磁碟機有限功能模式 (Network Drive Safe Mode) —")}{' '}
              {caps.remote_unc_path ?? capabilityPaths[index] ?? currentPath}
            </span>
          </div>
          <ul className="list-disc list-inside text-amber-200/90 space-y-0.5">
            {caps.limitations_notice.map((note, idx) => (
              <li key={idx}>{tx(note)}</li>
            ))}
          </ul>
        </div>
      ))}

      {/* 主內容區 */}
      <div className="flex-1 min-h-0 flex overflow-hidden">
        {/* 左側磁碟機與網路磁碟機狀態欄 */}
        {activeTab !== 'taskmgr' && (
        <aside className="w-72 border-r border-slate-800 bg-slate-900/50 p-4 flex flex-col gap-4 overflow-y-auto shrink-0">
          <div>
            <div className="flex items-center justify-between mb-2.5">
              <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                {t("儲存裝置與網路磁碟")}
              </span>
              <button
                onClick={() => keeperApi.getDrives().then(setDrives).catch(notifyError)}
                className="text-xs text-cyan-400 hover:text-cyan-300 flex items-center gap-1"
                title={t("重新整理磁碟清單")}
              >
                <RefreshCw className="w-3.5 h-3.5" />
              </button>
            </div>

            {activeTab === 'sync' && (
              <label className="block text-xs text-slate-400 mb-3">
                {t("選擇磁碟套用至")}
                <select
                  value={syncDriveSide}
                  onChange={(e) => setSyncDriveSide(e.target.value === 'right' ? 'right' : 'left')}
                  className="mt-1.5 w-full bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
                >
                  <option value="left">{t("左側來源目錄 (Source / Left)")}</option>
                  <option value="right">{t("右側目標目錄")}</option>
                </select>
              </label>
            )}

            <div className="space-y-2.5">
              {drives.map((d) => {
                const isNet = d.capabilities.is_network_drive;
                const isSelected = normalizePathKey(storagePath).startsWith(
                  `${normalizePathKey(d.mount_point)}\\`
                ) || normalizePathKey(storagePath) === normalizePathKey(d.mount_point);
                return (
                  <button
                    key={d.mount_point}
                    onClick={() => handleSelectDrive(d.mount_point)}
                    disabled={loading || (activeTab === 'treesize' && treeSizeLoading)}
                    aria-pressed={isSelected}
                    className={`w-full text-left p-3 rounded-xl border bg-slate-900/90 hover:border-cyan-500/50 transition-all group disabled:opacity-50 disabled:cursor-not-allowed ${
                      isSelected ? 'border-cyan-500/70' : 'border-slate-800'
                    }`}
                  >
                    <div className="flex items-center justify-between mb-1">
                      <div className="flex items-center gap-2">
                        {isNet ? (
                          <Network className="w-4 h-4 text-amber-400" />
                        ) : (
                          <HardDrive className="w-4 h-4 text-cyan-400" />
                        )}
                        <span className="font-semibold text-sm text-white">
                          {d.mount_point}
                        </span>
                      </div>
                      <span
                        className={`text-[10px] px-2 py-0.5 rounded-full font-medium ${
                          isNet
                            ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40'
                            : 'bg-slate-800 text-slate-300'
                        }`}
                      >
                        {isNet ? t("🌐 網路磁碟 (有限功能)") : tx(d.file_system)}
                      </span>
                    </div>
                    <div className="text-xs text-slate-400 truncate mb-2">
                      {tx(d.name)}
                    </div>
                    <div className="w-full h-1.5 bg-slate-800 rounded-full overflow-hidden mb-1">
                      <div
                        className={`h-full rounded-full ${
                          d.usage_percent > 90
                            ? 'bg-rose-500'
                            : isNet
                              ? 'bg-amber-400'
                              : 'bg-cyan-400'
                        }`}
                        style={{
                          width: `${Math.min(100, Math.max(3, d.usage_percent))}%`,
                        }}
                      />
                    </div>
                    <div className="flex justify-between text-[11px] text-slate-400">
                      <span>{t("已用 {pct}%", { pct: d.usage_percent.toFixed(1) })}</span>
                      <span>{t("可用 {size}", { size: formatBytes(d.available_bytes) })}</span>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* 網路磁碟機 (SMB/UNC) 有限功能策略檢測器 */}
          <div className="border-t border-slate-800 pt-4">
            <div className="flex items-center gap-1.5 text-xs font-semibold text-amber-300 mb-2">
              <Network className="w-4 h-4" />
              <span>{t("網路磁碟 (SMB/UNC) 權限檢測")}</span>
            </div>
            <p className="text-[11px] text-slate-400 mb-2">
              {t("輸入任何本機或遠端 UNC 路徑（如")}{' '}
              <code className="text-amber-200">\\NAS\Share</code>
              {t("），即時查驗系統套用的安全降級策略：")}
            </p>
            <div className="flex gap-1.5 mb-2">
              <input
                type="text"
                value={probeUncInput}
                onChange={(e) => setProbeUncInput(e.target.value)}
                className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-amber-500"
                placeholder={t("\\\\Server\\Share 或 C:\\")}
              />
              <button
                onClick={() => runProbeCapabilities(probeUncInput)}
                className="px-2.5 py-1.5 rounded-lg bg-amber-500/20 text-amber-300 border border-amber-500/40 hover:bg-amber-500/30 text-xs font-medium shrink-0"
              >
                {t("檢測")}
              </button>
            </div>

            {probedCaps && (
              <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-[11px] space-y-1.5">
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("識別類型：")}</span>
                  <span
                    className={`font-semibold ${
                      probedCaps.is_network_drive
                        ? 'text-amber-300'
                        : 'text-emerald-300'
                    }`}
                  >
                    {probedCaps.is_network_drive
                      ? t("🌐 網路磁碟 (有限功能)")
                      : t("💻 本機磁碟 (完整功能)")}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("本機資源回收筒：")}</span>
                  <span>
                    {probedCaps.supports_recycle_bin ? t("✅ 支援") : t("⚠️ 不支援 (直接刪除)")}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("MFT 極速掃描：")}</span>
                  <span>
                    {probedCaps.supports_mft_fast_scan
                      ? t("✅ 支援")
                      : t("⚠️ 停用 (受控執行緒)")}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("LockHunter 範圍：")}</span>
                  <span>
                    {probedCaps.lockhunter_scope === 'full_local'
                      ? t("✅ 完整本機行程")
                      : t("⚠️ 僅限本機 SMB 句柄")}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("圖片自動預覽上限：")}</span>
                  <span>{formatBytes(probedCaps.max_auto_preview_bytes)}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">{t("FreeFileSync 時間容差：")}</span>
                  <span>{t("{n} 秒", { n: probedCaps.sync_timestamp_tolerance_secs })}</span>
                </div>
              </div>
            )}
          </div>
        </aside>
        )}

        {/* 右側主工作面板 */}
        <main className="flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden">
          {/* ==================== 1. 檔案總管 ==================== */}
          {(
            <div className={activeTab === 'explorer' ? "flex-1 min-h-0 flex flex-col overflow-hidden" : "hidden"}>
              {/* 路徑導覽與工具列 */}
              <div className="p-3 border-b border-slate-800 bg-slate-900/40 flex flex-wrap items-center gap-2">
                <button
                  onClick={() =>
                    listing?.parent_path && loadDirectory(listing.parent_path)
                  }
                  disabled={!listing?.parent_path}
                  className="p-2 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-40 text-slate-200"
                  title={t("回上一層目錄")}
                >
                  <ArrowUp className="w-4 h-4" />
                </button>

                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    loadDirectory(pathInput);
                  }}
                  className="flex-1 flex gap-2 min-w-[260px]"
                >
                  <input
                    type="text"
                    value={pathInput}
                    onChange={(e) => setPathInput(e.target.value)}
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-3 py-1.5 text-xs text-slate-100 focus:outline-none focus:border-cyan-500"
                    placeholder={t("輸入本機路徑或 \\\\Server\\Share 網路磁碟路徑...")}
                  />
                  <button
                    type="submit"
                    className="px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs"
                  >
                    {t("前往")}
                  </button>
                </form>

                {/* 搜尋與圖片篩選 */}
                <div className="relative">
                  <Search className="w-3.5 h-3.5 text-slate-400 absolute left-2.5 top-2" />
                  <input
                    type="text"
                    value={searchFilter}
                    onChange={(e) => setSearchFilter(e.target.value)}
                    placeholder={t("過濾檔案名稱...")}
                    className="bg-slate-950 border border-slate-800 rounded-lg pl-8 pr-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-cyan-500 w-44"
                  />
                </div>
              </div>

              {/* 檔案清單 + 右側圖片檢視面板 */}
              <div className="flex-1 min-h-0 flex overflow-hidden">
                {/* 檔案表格 */}
                <div className="flex-1 min-w-0 min-h-0 overflow-y-auto p-3">
                  <div className="mb-2 flex items-center justify-between text-xs text-slate-400 px-2">
                    <span>
                      {t("共 {dirs} 個資料夾、{files} 個檔案（總計 {size}）", {
                        dirs: listing?.total_dirs ?? 0,
                        files: listing?.total_files ?? 0,
                        size: formatBytes(listing?.total_size_bytes ?? 0),
                      })}
                    </span>
                    {loading && (
                      <span className="text-cyan-400 animate-pulse">
                        {t("Rust 引擎讀取中...")}
                      </span>
                    )}
                  </div>

                  <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-900/40">
                    <table className="w-full text-left border-collapse text-xs table-fixed">
                      <thead>
                        <tr className="border-b border-slate-800 bg-slate-900/90 text-slate-400">
                          <th className="py-2.5 px-3 font-medium min-w-0">{t("名稱")}</th>
                          <th className="py-2.5 px-3 font-medium w-24 shrink-0">{t("類型")}</th>
                          <th className="py-2.5 px-3 font-medium w-28 text-right shrink-0">
                            {t("大小")}
                          </th>
                          <th className="py-2.5 px-3 font-medium w-48 shrink-0">
                            {t("修改時間")}
                          </th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-slate-800/60">
                        {filteredEntries.map((item) => {
                          const isSelected = selectedEntry?.path === item.path;
                          return (
                            <tr
                              key={item.path}
                              onClick={() => handleSelectEntry(item)}
                              onDoubleClick={() => {
                                if (item.is_dir) loadDirectory(item.path);
                              }}
                              className={`cursor-pointer transition-colors ${
                                isSelected
                                  ? 'bg-cyan-500/15 text-white'
                                  : 'hover:bg-slate-800/60 text-slate-200'
                              }`}
                            >
                              <td className="py-2.5 px-3 min-w-0 max-w-0 overflow-hidden">
                                <div className="flex items-center gap-2 min-w-0">
                                  {item.is_dir ? (
                                    <Folder className="w-4 h-4 text-amber-400 shrink-0" />
                                  ) : item.is_image ? (
                                    <ImageIcon className="w-4 h-4 text-pink-400 shrink-0" />
                                  ) : item.category === 'code' ? (
                                    <FileCode className="w-4 h-4 text-cyan-400 shrink-0" />
                                  ) : item.category === 'archive' ||
                                    ['zip', 'rar', '7z', 'tar', 'gz', 'jar'].includes(
                                      item.extension
                                    ) ? (
                                    <Archive className="w-4 h-4 text-purple-400 shrink-0" />
                                  ) : item.extension === 'pdf' ? (
                                    <FileText className="w-4 h-4 text-rose-400 shrink-0" />
                                  ) : item.category === 'video' ? (
                                    <Film className="w-4 h-4 text-teal-400 shrink-0" />
                                  ) : item.category === 'audio' ? (
                                    <Music className="w-4 h-4 text-emerald-400 shrink-0" />
                                  ) : (
                                    <FileText className="w-4 h-4 text-slate-400 shrink-0" />
                                  )}
                                  <span
                                    className="truncate min-w-0 select-text cursor-text font-medium text-slate-100"
                                    title={item.name}
                                  >
                                    {item.name}
                                  </span>
                                  {item.is_image && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-pink-500/20 text-pink-300 border border-pink-500/30 shrink-0">
                                      {t("圖片")}
                                    </span>
                                  )}
                                  {item.extension === 'pdf' && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-300 border border-rose-500/30 shrink-0">
                                      PDF
                                    </span>
                                  )}
                                  {(item.extension === 'zip' ||
                                    item.extension === 'jar') && (
                                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 border border-purple-500/30 shrink-0">
                                      {t("ZIP 壓縮")}
                                    </span>
                                  )}
                                </div>
                              </td>
                              <td className="py-2.5 px-3 text-slate-400 truncate whitespace-nowrap">
                                {item.is_dir
                                  ? t("資料夾")
                                  : item.extension.toUpperCase() || t("檔案")}
                              </td>
                              <td className="py-2.5 px-3 text-right font-mono text-slate-300 whitespace-nowrap">
                                {item.is_dir
                                  ? '-'
                                  : formatBytes(item.size_bytes)}
                              </td>
                              <td className="py-2.5 px-3 text-slate-400 whitespace-nowrap">
                                {formatTimestamp(item.modified_unix, locale)}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </div>

                {/* 右側項目預覽與詳情檢視面板 */}
                <div className="w-96 lg:w-[480px] xl:w-[520px] border-l border-slate-800 bg-slate-900/60 p-4 flex flex-col gap-4 overflow-y-auto shrink-0">
                  <div className="flex items-center justify-between pb-1 border-b border-slate-800/80">
                    <div className="flex items-center gap-2 font-semibold text-sm text-white">
                      {generalPreview?.preview_kind === 'directory' ? (
                        <>
                          <Folder className="w-4 h-4 text-amber-400" />
                          <span>{t("資料夾說明與概況")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'image' ? (
                        <>
                          <ImageIcon className="w-4 h-4 text-pink-400" />
                          <span>{t("圖片影像預覽")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'text' ? (
                        <>
                          <FileCode className="w-4 h-4 text-cyan-400" />
                          <span>{t("純文字與程式碼預覽")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'pdf' ? (
                        <>
                          <FileText className="w-4 h-4 text-rose-400" />
                          <span>{t("PDF 文件預覽")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'zip' ? (
                        <>
                          <Archive className="w-4 h-4 text-purple-400" />
                          <span>{t("ZIP 壓縮檔目錄清單")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'media' ? (
                        <>
                          <Film className="w-4 h-4 text-teal-400" />
                          <span>{t("影音媒體播放器")}</span>
                        </>
                      ) : generalPreview?.preview_kind === 'hex' ? (
                        <>
                          <Binary className="w-4 h-4 text-slate-400" />
                          <span>{t("二進位 Hex Dump 檢視")}</span>
                        </>
                      ) : (
                        <>
                          <Eye className="w-4 h-4 text-cyan-400" />
                          <span>{t("項目預覽與詳細資訊")}</span>
                        </>
                      )}
                    </div>

                    {/* 頂部操作工具按鈕 */}
                    <div className="flex items-center gap-1.5">
                      {generalPreview?.preview_kind === 'image' &&
                        imagePreview?.data_url && (
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() =>
                                setImageZoom((z) => Math.max(50, z - 25))
                              }
                              className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300"
                              title={t("縮小圖片")}
                            >
                              <ZoomOut className="w-3.5 h-3.5" />
                            </button>
                            <span className="text-[11px] text-slate-300 px-1 font-mono">
                              {imageZoom}%
                            </span>
                            <button
                              onClick={() =>
                                setImageZoom((z) => Math.min(200, z + 25))
                              }
                              className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-300"
                              title={t("放大圖片")}
                            >
                              <ZoomIn className="w-3.5 h-3.5" />
                            </button>
                          </div>
                        )}

                      {generalPreview?.preview_kind === 'text' &&
                        generalPreview.text_preview && (
                          <div className="flex items-center gap-1">
                            {(generalPreview.text_preview.syntax ===
                              'markdown' ||
                              generalPreview.text_preview.syntax === 'csv' ||
                              generalPreview.text_preview.syntax ===
                                'tsv') && (
                              <button
                                onClick={() =>
                                  setTextViewMode((m) =>
                                    m === 'raw' ? 'parsed' : 'raw'
                                  )
                                }
                                className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-cyan-300 border border-slate-700"
                              >
                                {textViewMode === 'raw'
                                  ? generalPreview.text_preview.syntax ===
                                    'markdown'
                                    ? t("排版預覽")
                                    : t("表格預覽")
                                  : t("原始碼")}
                              </button>
                            )}
                            <button
                              onClick={() => {
                                if (generalPreview?.text_preview?.content) {
                                  navigator.clipboard
                                    .writeText(
                                      generalPreview.text_preview.content
                                    )
                                    .then(() => {
                                      setCopiedText(true);
                                      setTimeout(
                                        () => setCopiedText(false),
                                        2000
                                      );
                                    });
                                }
                              }}
                              className="flex items-center gap-1 px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-slate-200 border border-slate-700"
                              title={t("複製文字內容")}
                            >
                              {copiedText ? (
                                <>
                                  <Check className="w-3 h-3 text-emerald-400" />
                                  <span className="text-emerald-400">
                                    {t("已複製")}
                                  </span>
                                </>
                              ) : (
                                <>
                                  <Copy className="w-3 h-3 text-slate-400" />
                                  <span>{t("複製")}</span>
                                </>
                              )}
                            </button>
                          </div>
                        )}

                      {generalPreview?.preview_kind === 'pdf' &&
                        generalPreview.pdf_preview?.data_url && (
                          <button
                            onClick={() => {
                              const w = window.open('');
                              w?.document.write(
                                `<iframe src="${generalPreview.pdf_preview?.data_url}" style="border:0; top:0; left:0; bottom:0; right:0; width:100%; height:100%;" allowfullscreen></iframe>`
                              );
                            }}
                            className="flex items-center gap-1 px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-[11px] text-rose-300 border border-slate-700"
                            title={t("另開新視窗全螢幕閱讀")}
                          >
                            <ExternalLink className="w-3 h-3" />
                            <span>{t("全螢幕")}</span>
                          </button>
                        )}
                    </div>
                  </div>

                  {/* 右方區域頂端快速操作工具列 */}
                  <div className="p-3 rounded-xl bg-slate-950/80 border border-slate-800 flex flex-wrap items-center justify-between gap-2 shrink-0">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider shrink-0">
                        {t("快速操作")}
                      </span>
                      <span
                        className="text-xs text-slate-300 font-medium truncate max-w-[180px] font-mono"
                        title={previewTarget.path}
                      >
                        {previewTarget.name}
                      </span>
                    </div>

                    <div className="flex items-center gap-1.5 flex-wrap">
                      {previewTarget.isDir && previewTarget.isEntry && (
                        <button
                          onClick={() => loadDirectory(previewTarget.path)}
                          className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs shadow-sm transition-colors"
                          title={t("開啟")}
                        >
                          <Folder className="w-3.5 h-3.5" />
                          <span>{t("開啟")}</span>
                        </button>
                      )}
                      {previewTarget.isDir && (
                        <button
                          onClick={() => {
                            setTreeSizePath(previewTarget.path);
                            setActiveTab('treesize');
                            runTreeSize(previewTarget.path);
                          }}
                          className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 hover:bg-indigo-500/30 text-xs font-medium transition-colors"
                          title={t("分析此目錄空間")}
                        >
                          <PieChart className="w-3.5 h-3.5" />
                          <span>{t("分析空間佔用")}</span>
                        </button>
                      )}
                      <button
                        onClick={() => openLockModal(previewTarget.path)}
                        className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-amber-500/20 text-amber-300 border border-amber-500/30 hover:bg-amber-500/30 text-xs font-medium transition-colors"
                        title={t("使用 LockHunter 查詢誰鎖住了此項目")}
                      >
                        <Unlock className="w-3.5 h-3.5" />
                        <span>{t("解鎖")}</span>
                      </button>
                      {previewTarget.isEntry && (
                        <button
                          onClick={() => {
                            if (selectedEntry) handleDeleteEntry(selectedEntry);
                          }}
                          className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-rose-500/20 text-rose-300 border border-rose-500/30 hover:bg-rose-500/30 text-xs font-medium transition-colors"
                          title={t("刪除項目")}
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                          <span>{t("刪除")}</span>
                        </button>
                      )}
                    </div>
                  </div>

                  {/* 載入中骨架 */}
                  {previewLoading && (
                    <div className="p-4 rounded-xl bg-slate-950/60 border border-slate-800/80 text-xs text-cyan-400 animate-pulse flex items-center gap-2">
                      <RefreshCw className="w-4 h-4 animate-spin" />
                      <span>{t("正在讀取並解析項目預覽...")}</span>
                    </div>
                  )}

                  {/* ===================== 1. 資料夾說明預覽 ===================== */}
                  {generalPreview?.preview_kind === 'directory' &&
                    generalPreview.dir_preview && (
                      <div className="space-y-3.5">
                        <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 space-y-3">
                          <div className="flex items-start gap-3">
                            <div className="w-10 h-10 rounded-xl bg-amber-500/15 border border-amber-500/30 flex items-center justify-center shrink-0">
                              <Folder className="w-5 h-5 text-amber-400" />
                            </div>
                            <div className="flex-1 min-w-0">
                              <div className="font-semibold text-white truncate text-sm">
                                {generalPreview.dir_preview.name}
                              </div>
                              <div className="text-[11px] text-slate-400 break-all select-all font-mono">
                                {generalPreview.dir_preview.path}
                              </div>
                            </div>
                          </div>

                          <div className="flex flex-wrap items-center gap-1.5 pt-1">
                            <span
                              className={`text-[10px] px-2 py-0.5 rounded font-medium border ${
                                generalPreview.dir_preview.badge_color ===
                                'rose'
                                  ? 'bg-rose-500/20 text-rose-300 border-rose-500/40'
                                  : generalPreview.dir_preview.badge_color ===
                                      'cyan'
                                    ? 'bg-cyan-500/20 text-cyan-300 border-cyan-500/40'
                                    : generalPreview.dir_preview.badge_color ===
                                        'emerald'
                                      ? 'bg-emerald-500/20 text-emerald-300 border-emerald-500/40'
                                      : generalPreview.dir_preview
                                            .badge_color === 'purple'
                                        ? 'bg-purple-500/20 text-purple-300 border-purple-500/40'
                                        : generalPreview.dir_preview
                                              .badge_color === 'amber'
                                          ? 'bg-amber-500/20 text-amber-300 border-amber-500/40'
                                          : generalPreview.dir_preview
                                                .badge_color === 'blue'
                                            ? 'bg-blue-500/20 text-blue-300 border-blue-500/40'
                                            : 'bg-slate-800 text-slate-300 border-slate-700'
                              }`}
                            >
                              {tx(generalPreview.dir_preview.category_label)}
                            </span>
                            {generalPreview.dir_preview.is_system_or_known && (
                              <span className="text-[10px] px-1.5 py-0.5 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 flex items-center gap-1">
                                <ShieldCheck className="w-3 h-3" />
                                {t("系統/常見目錄")}
                              </span>
                            )}
                          </div>
                        </div>

                        {/* 用途說明卡片 */}
                        <div className="p-3.5 rounded-xl bg-slate-950/90 border border-slate-800 space-y-2 text-xs">
                          <div className="font-semibold text-slate-200 flex items-center gap-1.5">
                            <BookOpen className="w-4 h-4 text-cyan-400" />
                            <span>{t("目錄功能與用途介紹")}</span>
                          </div>
                          <div className="text-slate-300 leading-relaxed font-medium">
                            {tx(generalPreview.dir_preview.summary)}
                          </div>
                          <p className="text-slate-400 leading-relaxed text-[11px] pt-1 border-t border-slate-800/80">
                            {tx(generalPreview.dir_preview.description)}
                          </p>
                        </div>

                        {/* 安全性警告指引 */}
                        {generalPreview.dir_preview.safety_warning && (
                          <div className="p-3 rounded-xl bg-amber-950/40 border border-amber-700/50 text-xs text-amber-200 flex items-start gap-2.5">
                            <ShieldAlert className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
                            <div className="leading-relaxed">
                              <span className="font-semibold block mb-0.5">
                                {t("安全與維護指引：")}
                              </span>
                              <span>
                                {tx(generalPreview.dir_preview.safety_warning)}
                              </span>
                            </div>
                          </div>
                        )}

                        {/* 直屬項目統計 */}
                        <div className="grid grid-cols-3 gap-2">
                          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-center">
                            <div className="text-[11px] text-slate-400">
                              {t("直屬子資料夾")}
                            </div>
                            <div className="font-semibold text-white text-sm mt-0.5">
                              {generalPreview.dir_preview.direct_subdirs_count}
                            </div>
                          </div>
                          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-center">
                            <div className="text-[11px] text-slate-400">
                              {t("直屬檔案數")}
                            </div>
                            <div className="font-semibold text-white text-sm mt-0.5">
                              {generalPreview.dir_preview.direct_files_count}
                            </div>
                          </div>
                          <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-center">
                            <div className="text-[11px] text-slate-400">
                              {t("直屬大小")}
                            </div>
                            <div className="font-semibold text-cyan-300 text-xs mt-1">
                              {formatBytes(
                                generalPreview.dir_preview.direct_size_bytes
                              )}
                            </div>
                          </div>
                        </div>
                      </div>
                    )}

                  {/* ===================== 2. 圖片影像預覽 ===================== */}
                  {generalPreview?.preview_kind === 'image' && imagePreview && (
                    <div className="space-y-3">
                      {imagePreview.deferred_for_network ? (
                        <div className="p-4 rounded-xl bg-amber-950/50 border border-amber-700/60 text-xs text-amber-200 space-y-3">
                          <div className="font-semibold flex items-center gap-1.5">
                            <ShieldAlert className="w-4 h-4 text-amber-400" />
                            <span>{t("網路磁碟機大圖保護已觸發")}</span>
                          </div>
                          <p>{tx(imagePreview.notice)}</p>
                          {selectedEntry && (
                            <button
                              onClick={() =>
                                handleSelectEntry(selectedEntry, true)
                              }
                              className="w-full py-2 rounded-lg bg-amber-500 text-slate-950 font-semibold hover:bg-amber-400"
                            >
                              {t("強制跨網路載入原圖預覽")}
                            </button>
                          )}
                        </div>
                      ) : (
                        imagePreview.data_url && (
                          <div className="rounded-xl border border-slate-800 bg-slate-950/90 p-3 flex items-center justify-center overflow-auto max-h-80">
                            <img
                              src={imagePreview.data_url}
                              alt={imagePreview.name}
                              style={{
                                transform: `scale(${imageZoom / 100})`,
                                transformOrigin: 'center center',
                              }}
                              className="max-w-full max-h-72 object-contain rounded transition-transform"
                            />
                          </div>
                        )
                      )}

                      <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-1.5">
                        <div className="font-semibold text-white truncate">
                          {imagePreview.name}
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>{t("解析度尺寸：")}</span>
                          <span className="text-slate-200 font-mono">
                            {imagePreview.width} × {imagePreview.height} px (
                            {imagePreview.aspect_ratio})
                          </span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>{t("影像格式 / 色彩：")}</span>
                          <span className="text-slate-200">
                            {imagePreview.format_name} (
                            {imagePreview.color_type})
                          </span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>{t("檔案大小：")}</span>
                          <span className="text-slate-200 font-mono">
                            {formatBytes(imagePreview.file_size_bytes)}
                          </span>
                        </div>
                      </div>
                    </div>
                  )}

                  {/* ===================== 3. 純文字與程式碼預覽 ===================== */}
                  {generalPreview?.preview_kind === 'text' &&
                    generalPreview.text_preview && (
                      <div className="space-y-3">
                        <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-1.5">
                          <div className="flex items-center justify-between">
                            <div className="font-semibold text-white truncate max-w-[240px]">
                              {generalPreview.name}
                            </div>
                            <span className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-500/20 text-cyan-300 border border-cyan-500/30 uppercase font-mono">
                              {generalPreview.text_preview.syntax}
                            </span>
                          </div>
                          <div className="flex justify-between text-slate-400 pt-0.5">
                            <span>{t("行數與大小：")}</span>
                            <span className="text-slate-200 font-mono">
                              {t("{lines} 行 · {size}", {
                                lines: generalPreview.text_preview.total_lines,
                                size: formatBytes(generalPreview.file_size_bytes),
                              })}
                            </span>
                          </div>
                        </div>

                        {generalPreview.notice && (
                          <div className="p-2.5 rounded-lg bg-amber-950/40 border border-amber-700/50 text-[11px] text-amber-200">
                            {tx(generalPreview.notice)}
                          </div>
                        )}

                        {/* 程式碼與文字檢視框 */}
                        {textViewMode === 'parsed' &&
                        generalPreview.text_preview.syntax === 'markdown' ? (
                          <div className="rounded-xl border border-slate-800 bg-slate-950 p-4 max-h-96 overflow-y-auto text-xs text-slate-200 leading-relaxed space-y-2">
                            {generalPreview.text_preview.content
                              .split('\n')
                              .map((line, idx) => {
                                if (line.startsWith('# ')) {
                                  return (
                                    <h1
                                      key={idx}
                                      className="text-base font-bold text-white border-b border-slate-800 pb-1"
                                    >
                                      {line.slice(2)}
                                    </h1>
                                  );
                                }
                                if (line.startsWith('## ')) {
                                  return (
                                    <h2
                                      key={idx}
                                      className="text-sm font-semibold text-cyan-300 pt-1"
                                    >
                                      {line.slice(3)}
                                    </h2>
                                  );
                                }
                                if (line.startsWith('### ')) {
                                  return (
                                    <h3
                                      key={idx}
                                      className="text-xs font-semibold text-slate-200"
                                    >
                                      {line.slice(4)}
                                    </h3>
                                  );
                                }
                                if (
                                  line.startsWith('- ') ||
                                  line.startsWith('* ')
                                ) {
                                  return (
                                    <li
                                      key={idx}
                                      className="ml-4 list-disc text-slate-300"
                                    >
                                      {line.slice(2)}
                                    </li>
                                  );
                                }
                                if (line.trim().startsWith('>')) {
                                  return (
                                    <blockquote
                                      key={idx}
                                      className="border-l-2 border-cyan-500 pl-2 text-slate-400 italic"
                                    >
                                      {line.replace(/^>\s*/, '')}
                                    </blockquote>
                                  );
                                }
                                return (
                                  <p
                                    key={idx}
                                    className="text-slate-300 min-h-[1em]"
                                  >
                                    {line}
                                  </p>
                                );
                              })}
                          </div>
                        ) : textViewMode === 'parsed' &&
                          (generalPreview.text_preview.syntax === 'csv' ||
                            generalPreview.text_preview.syntax === 'tsv') ? (
                          <div className="rounded-xl border border-slate-800 bg-slate-950 p-2 max-h-96 overflow-auto">
                            <table className="w-full text-[11px] border-collapse">
                              <tbody>
                                {generalPreview.text_preview.content
                                  .split('\n')
                                  .slice(0, 40)
                                  .filter((l) => l.trim().length > 0)
                                  .map((line, rowIdx) => {
                                    const sep =
                                      generalPreview?.text_preview?.syntax ===
                                      'tsv'
                                        ? '\t'
                                        : ',';
                                    const cols = line.split(sep);
                                    return (
                                      <tr
                                        key={rowIdx}
                                        className={
                                          rowIdx === 0
                                            ? 'bg-slate-900 font-semibold text-cyan-300'
                                            : 'hover:bg-slate-900/50 text-slate-300 border-b border-slate-900'
                                        }
                                      >
                                        {cols.map((col, colIdx) => (
                                          <td
                                            key={colIdx}
                                            className="p-1.5 border border-slate-800 truncate max-w-[120px]"
                                          >
                                            {col.trim()}
                                          </td>
                                        ))}
                                      </tr>
                                    );
                                  })}
                              </tbody>
                            </table>
                          </div>
                        ) : (
                          <div className="rounded-xl border border-slate-800 bg-slate-950 p-2.5 max-h-96 overflow-auto font-mono text-[11px]">
                            <table className="w-full border-collapse">
                              <tbody>
                                {generalPreview.text_preview.content
                                  .split('\n')
                                  .map((line, idx) => (
                                    <tr
                                      key={idx}
                                      className="hover:bg-slate-900/60 leading-5"
                                    >
                                      <td className="w-10 select-none text-right pr-3 text-slate-600 shrink-0 font-mono text-[10px]">
                                        {idx + 1}
                                      </td>
                                      <td className="text-slate-300 whitespace-pre break-all">
                                        {line || ' '}
                                      </td>
                                    </tr>
                                  ))}
                              </tbody>
                            </table>
                          </div>
                        )}
                      </div>
                    )}

                  {/* ===================== 4. PDF 文件預覽 ===================== */}
                  {generalPreview?.preview_kind === 'pdf' &&
                    generalPreview.pdf_preview && (
                      <div className="space-y-3">
                        <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-1.5">
                          <div className="flex items-center justify-between">
                            <span className="font-semibold text-white truncate max-w-[240px]">
                              {generalPreview.name}
                            </span>
                            <span className="text-[10px] px-1.5 py-0.5 rounded bg-rose-500/20 text-rose-300 border border-rose-500/30 uppercase">
                              {generalPreview.pdf_preview.version}
                            </span>
                          </div>
                          <div className="flex justify-between text-slate-400">
                            <span>{t("文件大小：")}</span>
                            <span className="text-slate-200 font-mono">
                              {formatBytes(
                                generalPreview.pdf_preview.file_size_bytes
                              )}
                            </span>
                          </div>
                        </div>

                        {generalPreview.pdf_preview.deferred_for_network ? (
                          <div className="p-4 rounded-xl bg-amber-950/40 border border-amber-700/60 text-xs text-amber-200 space-y-3">
                            <div className="font-semibold flex items-center gap-1.5">
                              <ShieldAlert className="w-4 h-4 text-amber-400" />
                              <span>{t("PDF 大檔保護已觸發")}</span>
                            </div>
                            <p>{tx(generalPreview.pdf_preview.notice)}</p>
                            {selectedEntry && (
                              <button
                                onClick={() =>
                                  handleSelectEntry(selectedEntry, true)
                                }
                                className="w-full py-2 rounded-lg bg-rose-500 text-white font-semibold hover:bg-rose-400"
                              >
                                {t("強制載入 PDF 原文預覽")}
                              </button>
                            )}
                          </div>
                        ) : (
                          generalPreview.pdf_preview.data_url && (
                            <div className="rounded-xl border border-slate-800 overflow-hidden bg-slate-950">
                              <iframe
                                src={generalPreview.pdf_preview.data_url}
                                title={generalPreview.name}
                                className="w-full h-96 border-none bg-slate-900"
                              />
                            </div>
                          )
                        )}
                      </div>
                    )}

                  {/* ===================== 5. ZIP 壓縮檔預覽 ===================== */}
                  {generalPreview?.preview_kind === 'zip' &&
                    generalPreview.zip_preview && (
                      <div className="space-y-3">
                        <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-2">
                          <div className="font-semibold text-white truncate">
                            {generalPreview.name}
                          </div>
                          <div className="grid grid-cols-2 gap-2 pt-1 border-t border-slate-800 text-[11px]">
                            <div>
                              <span className="text-slate-400">{t("項目總數：")}</span>
                              <span className="text-slate-200 font-medium ml-1">
                                {t("{files} 檔案 · {dirs} 目錄", {
                                  files: generalPreview.zip_preview.total_files,
                                  dirs: generalPreview.zip_preview.total_dirs,
                                })}
                              </span>
                            </div>
                            <div>
                              <span className="text-slate-400">{t("整體節省：")}</span>
                              <span className="text-emerald-400 font-bold ml-1">
                                {generalPreview.zip_preview.overall_ratio.toFixed(
                                  1
                                )}
                                %
                              </span>
                            </div>
                            <div>
                              <span className="text-slate-400">{t("原始總量：")}</span>
                              <span className="text-slate-300 font-mono ml-1">
                                {formatBytes(
                                  generalPreview.zip_preview
                                    .total_uncompressed_bytes
                                )}
                              </span>
                            </div>
                            <div>
                              <span className="text-slate-400">{t("壓縮大小：")}</span>
                              <span className="text-purple-300 font-mono ml-1">
                                {formatBytes(
                                  generalPreview.zip_preview
                                    .total_compressed_bytes
                                )}
                              </span>
                            </div>
                          </div>
                          {generalPreview.zip_preview.comment && (
                            <div className="text-[10px] text-slate-400 italic pt-1">
                              {t("註解：{comment}", { comment: generalPreview.zip_preview.comment })}
                            </div>
                          )}
                        </div>

                        {generalPreview.notice && (
                          <div className="p-2.5 rounded-lg bg-amber-950/40 border border-amber-700/50 text-[11px] text-amber-200">
                            {tx(generalPreview.notice)}
                          </div>
                        )}

                        {/* 壓縮檔內部檔案列表 */}
                        <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-950 max-h-72 overflow-y-auto">
                          <table className="w-full text-[11px] text-left border-collapse">
                            <thead>
                              <tr className="bg-slate-900 text-slate-400 border-b border-slate-800 sticky top-0">
                                <th className="p-2 font-medium">{t("內部名稱")}</th>
                                <th className="p-2 font-medium text-right w-20">
                                  {t("原始大小")}
                                </th>
                                <th className="p-2 font-medium text-right w-16">
                                  {t("壓縮率")}
                                </th>
                              </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-800/60">
                              {generalPreview.zip_preview.entries.map(
                                (item, i) => (
                                  <tr
                                    key={i}
                                    className="hover:bg-slate-900/50 text-slate-300"
                                  >
                                    <td className="p-1.5 flex items-center gap-1.5 truncate max-w-[200px]">
                                      {item.is_dir ? (
                                        <Folder className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                                      ) : (
                                        <FileText className="w-3.5 h-3.5 text-slate-400 shrink-0" />
                                      )}
                                      <span className="truncate">
                                        {item.name}
                                      </span>
                                    </td>
                                    <td className="p-1.5 text-right font-mono text-slate-400">
                                      {item.is_dir
                                        ? '-'
                                        : formatBytes(item.uncompressed_size)}
                                    </td>
                                    <td className="p-1.5 text-right font-mono text-emerald-400">
                                      {item.is_dir
                                        ? '-'
                                        : `${item.compression_ratio.toFixed(0)}%`}
                                    </td>
                                  </tr>
                                )
                              )}
                            </tbody>
                          </table>
                        </div>
                      </div>
                    )}

                  {/* ===================== 6. 影音媒體預覽 ===================== */}
                  {generalPreview?.preview_kind === 'media' &&
                    generalPreview.media_preview && (
                      <div className="space-y-3">
                        <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-1.5">
                          <div className="font-semibold text-white truncate">
                            {generalPreview.name}
                          </div>
                          <div className="flex justify-between text-slate-400">
                            <span>{t("媒體類型 / 格式：")}</span>
                            <span className="text-teal-300 font-medium">
                              {generalPreview.media_preview.media_type ===
                              'audio'
                                ? t("音訊檔案")
                                : t("視訊影片")}{' '}
                              ({generalPreview.media_preview.format})
                            </span>
                          </div>
                          <div className="flex justify-between text-slate-400">
                            <span>{t("檔案大小：")}</span>
                            <span className="text-slate-200 font-mono">
                              {formatBytes(generalPreview.file_size_bytes)}
                            </span>
                          </div>
                        </div>

                        {generalPreview.media_preview.data_url && (
                          <div className="p-4 rounded-xl border border-slate-800 bg-slate-950 flex flex-col items-center justify-center">
                            {generalPreview.media_preview.media_type ===
                            'audio' ? (
                              <div className="w-full space-y-3 text-center">
                                <Music className="w-10 h-10 text-teal-400 mx-auto" />
                                <audio
                                  controls
                                  src={generalPreview.media_preview.data_url}
                                  className="w-full"
                                />
                              </div>
                            ) : (
                              <video
                                controls
                                src={generalPreview.media_preview.data_url}
                                className="w-full max-h-72 rounded-lg bg-black"
                              />
                            )}
                          </div>
                        )}
                      </div>
                    )}

                  {/* ===================== 7. 二進位 Hex Dump 預覽 ===================== */}
                  {generalPreview?.preview_kind === 'hex' &&
                    generalPreview.hex_preview && (
                      <div className="space-y-3">
                        <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-2">
                          <div className="font-semibold text-white truncate">
                            {generalPreview.name}
                          </div>
                          <div className="flex justify-between text-slate-400">
                            <span>{t("檔案大小：")}</span>
                            <span className="text-slate-200 font-mono">
                              {formatBytes(generalPreview.file_size_bytes)}
                            </span>
                          </div>
                          {generalPreview.hex_preview.magic_description && (
                            <div className="p-2 rounded bg-cyan-950/40 border border-cyan-800/60 text-[11px] text-cyan-200">
                              <span className="font-semibold block mb-0.5">
                                {t("檔案特徵碼分析：")}
                              </span>
                              <span>
                                {tx(generalPreview.hex_preview.magic_description)}
                              </span>
                            </div>
                          )}
                        </div>

                        {/* Hex Dump 視窗 */}
                        <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 font-mono text-[10px] max-h-80 overflow-auto">
                          <table className="w-full border-collapse">
                            <thead>
                              <tr className="text-slate-500 border-b border-slate-800 pb-1 select-none text-left">
                                <th className="w-16">Offset</th>
                                <th>Hex</th>
                                <th className="w-24 text-right">ASCII</th>
                              </tr>
                            </thead>
                            <tbody>
                              {generalPreview.hex_preview.rows.map((row, i) => (
                                <tr
                                  key={i}
                                  className="hover:bg-slate-900/60 leading-5"
                                >
                                  <td className="text-cyan-500/80 font-mono select-none">
                                    {row.offset}
                                  </td>
                                  <td className="text-slate-300 font-mono tracking-wider whitespace-pre px-2">
                                    {row.hex}
                                  </td>
                                  <td className="text-emerald-400/90 font-mono text-right whitespace-pre">
                                    {row.ascii}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      </div>
                    )}

                  {/* 空狀態 */}
                  {!generalPreview && !previewLoading && (
                    <div className="p-6 rounded-xl border border-dashed border-slate-800 text-center text-xs text-slate-400 space-y-2">
                      <Folder className="w-8 h-8 text-slate-600 mx-auto" />
                      <p>
                        {t("從左側列表點選任何資料夾或檔案，即可檢視即時預覽。")}
                      </p>
                      <p className="text-[11px] text-slate-500">
                        {t("支援系統與常見資料夾詳細說明、圖片、純文字程式碼、PDF 文件、ZIP 壓縮檔、影音媒體與二進位 Hex Dump。")}
                      </p>
                    </div>
                  )}

                  {/* 網路磁碟機永久刪除確認開關 */}
                  {listing?.capabilities.is_network_drive && (
                    <label className="flex items-start gap-2 p-3 rounded-xl bg-amber-950/40 border border-amber-800/60 text-xs text-amber-200 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={confirmNetworkPermanentDelete}
                        onChange={(e) =>
                          setConfirmNetworkPermanentDelete(e.target.checked)
                        }
                        className="mt-0.5"
                      />
                      <span>
                        {t("我了解網路磁碟機不支援本機資源回收筒，允許在網路磁碟上執行永久刪除。")}
                      </span>
                    </label>
                  )}
                </div>

              </div>
            </div>
          )}

          {/* ==================== 2. TreeSize 磁碟空間分析與 Treemap ==================== */}
          {(
            <div className={activeTab === 'treesize' ? "flex-1 overflow-y-auto p-5 space-y-5" : "hidden"}>
              <div className="flex flex-wrap items-center justify-between gap-3 bg-slate-900/60 p-4 rounded-2xl border border-slate-800">
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (!treeSizeLoading) runTreeSize(treeSizePath, true);
                  }}
                  className="flex-1 flex gap-2 min-w-[280px]"
                >
                  <button
                    type="button"
                    onClick={handleTreeSizeGoParent}
                    disabled={!treeSizeParentPath || treeSizeLoading}
                    className="p-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 disabled:opacity-30 disabled:cursor-not-allowed text-slate-200 transition-colors shrink-0 flex items-center justify-center cursor-pointer"
                    title={t("回上一層目錄")}
                  >
                    <ArrowUp className="w-4 h-4" />
                  </button>
                  <input
                    type="text"
                    value={treeSizePath}
                    disabled={treeSizeLoading}
                    onChange={(e) => updateTreeSizePath(e.target.value)}
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100 disabled:opacity-60"
                    placeholder={t("輸入要分析空間的目錄路徑...")}
                  />
                  <button
                    type="submit"
                    disabled={treeSizeLoading}
                    className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 disabled:opacity-60 disabled:cursor-not-allowed text-slate-950 font-semibold text-xs flex items-center gap-1.5 shrink-0 cursor-pointer transition-colors shadow-md shadow-cyan-500/10"
                  >
                    {treeSizeLoading ? (
                      <RefreshCw className="w-4 h-4 animate-spin" />
                    ) : (
                      <PieChart className="w-4 h-4" />
                    )}
                    <span>{treeSizeLoading ? t("掃描分析中...") : t("立即掃描分析")}</span>
                  </button>
                </form>

                {!treeSizeLoading && treeSizeReport && (
                  <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
                    <div>
                      {treeSizeReport.is_drive_root && treeSizeReport.drive_total_bytes ? (
                        <>
                          <span className="text-slate-400">{t("磁碟容量：")}</span>
                          <span className="font-bold text-cyan-300 font-mono">
                            {formatBytes(treeSizeReport.drive_used_bytes ?? 0)}
                          </span>
                          <span className="text-slate-500 font-mono text-[11px]">
                            {' '}/ {formatBytes(treeSizeReport.drive_total_bytes)}
                          </span>
                          {treeSizeReport.drive_free_bytes != null && (
                            <span className="text-emerald-400 font-mono text-[11px] ml-1.5">
                              ({t("可用 {size}", { size: formatBytes(treeSizeReport.drive_free_bytes) })})
                            </span>
                          )}
                          <span className="text-slate-400 ml-3">{t("已掃描檔案：")}</span>
                        </>
                      ) : (
                        <span className="text-slate-400">{t("已分析大小：")}</span>
                      )}
                      <span className="font-bold text-cyan-300">
                        {formatBytes(treeSizeReport.total_size_bytes)}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-400">{t("檔案數：")}</span>
                      <span className="font-semibold text-white">
                        {treeSizeReport.total_files.toLocaleString()}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-400">{t("耗時：")}</span>
                      <span className="font-mono text-emerald-300">
                        {treeSizeReport.scan_duration_ms} ms
                      </span>
                    </div>
                  </div>
                )}
              </div>

              {(vhdxJob?.state === 'running' || vhdxJob?.recovery_required) && (
                <div className="flex items-center gap-2 p-3.5 rounded-xl bg-cyan-950/40 border border-cyan-600/40 text-cyan-200 text-xs">
                  <RefreshCw className="w-4 h-4 text-cyan-400 animate-spin shrink-0" />
                  <span>{vhdxJob.recovery_required ? t("環境尚未完全復原。可在此重試啟動，不會再次壓縮磁碟。") : t("虛擬磁碟壓縮進行中，請勿啟動 WSL / Docker...")}</span>
                  <span className="font-mono text-slate-400 truncate">{vhdxJob.path}</span>
                  <button type="button" onClick={() => handleCompactVhdx(vhdxJob.path, vhdxJob.path)} className="shrink-0 underline">{t("開啟壓縮精靈")}</button>
                </div>
              )}

              {/* 掃描進行中狀態（隱藏下方舊結果並呈現即時動態進度） */}
              {treeSizeLoading && (
                <div className="bg-slate-900/80 border border-cyan-500/30 rounded-2xl p-6 sm:p-8 flex flex-col items-center justify-center text-center space-y-6 shadow-xl shadow-cyan-950/20 backdrop-blur-sm">
                  <div className="flex flex-col items-center space-y-3">
                    <div className="relative flex items-center justify-center">
                      <div className="absolute w-20 h-20 rounded-full bg-cyan-500/20 animate-ping" />
                      <div className="w-16 h-16 rounded-2xl bg-slate-950 border border-cyan-500/50 flex items-center justify-center text-cyan-400 shadow-lg shadow-cyan-950/50">
                        <PieChart className="w-8 h-8 animate-pulse text-cyan-400" />
                      </div>
                    </div>
                    <div className="space-y-1 max-w-lg">
                      <h3 className="text-base sm:text-lg font-semibold text-white flex items-center justify-center gap-2">
                        <RefreshCw className="w-4 h-4 text-cyan-400 animate-spin" />
                        <span>{t("正在掃描磁碟目錄空間...")}</span>
                      </h3>
                      <p className="text-xs text-slate-400 break-all font-mono px-3 py-1.5 bg-slate-950/80 rounded-lg border border-slate-800 inline-block">
                        {treeSizeScanTarget || treeSizePath}
                      </p>
                    </div>
                  </div>

                  {/* 進度條與預估百分比 */}
                  <div className="w-full max-w-xl space-y-2">
                    <div className="w-full h-3 bg-slate-950 rounded-full overflow-hidden border border-slate-800 relative p-0.5 shadow-inner">
                      {treeSizeProgress?.estimated_percent != null ? (
                        <div
                          className="h-full bg-gradient-to-r from-cyan-500 via-sky-400 to-emerald-400 rounded-full transition-all duration-200 relative overflow-hidden"
                          style={{
                            width: `${Math.min(100, Math.max(3, treeSizeProgress.estimated_percent))}%`,
                          }}
                        >
                          <div className="absolute inset-0 bg-white/20 animate-pulse" />
                        </div>
                      ) : (
                        <div className="h-full bg-gradient-to-r from-cyan-500 via-sky-400 to-cyan-500 rounded-full w-full animate-pulse" />
                      )}
                    </div>
                    <div className="flex justify-between items-center text-xs text-slate-400 px-1 font-mono">
                      <span>
                        {treeSizeProgress?.estimated_percent != null
                          ? t("預估進度：{percent}%", {
                              percent: treeSizeProgress.estimated_percent.toFixed(1),
                            })
                          : t("遞迴遍歷子資料夾與統計大小中...")}
                      </span>
                      <span className="text-cyan-300 font-medium">
                        {t("已耗時 {n} 秒", {
                          n: treeSizeProgress
                            ? (treeSizeProgress.elapsed_ms / 1000).toFixed(1)
                            : scanElapsedSecs,
                        })}
                      </span>
                    </div>
                  </div>

                  {/* 即時 4 大指標數據卡片 */}
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 w-full max-w-xl">
                    <div className="bg-slate-950/70 border border-slate-800/80 rounded-xl p-3 flex flex-col items-center">
                      <span className="text-[11px] text-slate-400 mb-0.5">{t("已掃描檔案")}</span>
                      <span className="font-mono text-base font-bold text-white tracking-tight">
                        {(treeSizeProgress?.processed_files ?? 0).toLocaleString()}
                      </span>
                    </div>
                    <div className="bg-slate-950/70 border border-slate-800/80 rounded-xl p-3 flex flex-col items-center">
                      <span className="text-[11px] text-slate-400 mb-0.5">{t("已掃描資料夾")}</span>
                      <span className="font-mono text-base font-bold text-slate-200 tracking-tight">
                        {(treeSizeProgress?.processed_dirs ?? 0).toLocaleString()}
                      </span>
                    </div>
                    <div className="bg-slate-950/70 border border-slate-800/80 rounded-xl p-3 flex flex-col items-center">
                      <span className="text-[11px] text-slate-400 mb-0.5">{t("已統計大小")}</span>
                      <span className="font-mono text-base font-bold text-cyan-300 tracking-tight">
                        {formatBytes(treeSizeProgress?.processed_bytes ?? 0)}
                      </span>
                    </div>
                    <div className="bg-slate-950/70 border border-slate-800/80 rounded-xl p-3 flex flex-col items-center">
                      <span className="text-[11px] text-slate-400 mb-0.5">{t("分析速度")}</span>
                      <span className="font-mono text-base font-bold text-emerald-400 tracking-tight">
                        {treeSizeScanRate > 0
                          ? t("{rate} 檔/秒", { rate: treeSizeScanRate.toLocaleString() })
                          : "..."}
                      </span>
                    </div>
                  </div>

                  {/* 即時走訪路徑指示 */}
                  <div className="w-full max-w-xl bg-slate-950/90 border border-slate-800/90 rounded-xl px-3.5 py-2 flex items-center gap-2 text-left text-xs">
                    <Gauge className="w-3.5 h-3.5 text-cyan-400 shrink-0 animate-spin" />
                    <span className="text-slate-500 shrink-0 font-medium">{t("當前路徑：")}</span>
                    <span className="font-mono text-slate-300 truncate">
                      {treeSizeProgress?.current_path || treeSizeScanTarget || treeSizePath}
                    </span>
                  </div>

                  {/* 取消按鈕 */}
                  <div className="pt-1 flex items-center justify-center">
                    <button
                      type="button"
                      onClick={handleCancelTreeSize}
                      className="inline-flex items-center gap-2 px-5 py-2 rounded-xl bg-rose-950/40 hover:bg-rose-900/60 border border-rose-600/50 hover:border-rose-500 text-rose-200 hover:text-white text-xs font-semibold shadow-md transition-all cursor-pointer"
                    >
                      <XCircle className="w-4 h-4 text-rose-400" />
                      <span>{t("取消分析")}</span>
                    </button>
                  </div>

                  <p className="text-[11px] text-slate-500 max-w-md">
                    {t("💡 提示：已自動略過 Windows Junction 循環與系統虛擬保護區，保障掃描順暢與安全。")}
                  </p>
                </div>
              )}

              {/* 掃描完成後呈現分析結果 */}
              {!treeSizeLoading && treeSizeReport && (
                <>
                  {treeSizeReport.truncated_by_safety_limit && (
                    <div className="flex items-center gap-2 p-3.5 rounded-xl bg-amber-950/40 border border-amber-600/40 text-amber-200 text-xs">
                      <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
                      <span>{t("⚠️ 掃描已達項目安全上限，部分深層子目錄未完成統計。")}</span>
                    </div>
                  )}
                  {/* 互動式 Treemap 矩形樹狀圖 */}
                  <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                    <div className="flex items-center justify-between flex-wrap gap-2">
                      <div className="flex items-center gap-2.5">
                        <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                          <Layers className="w-4 h-4 text-cyan-400" />
                          {t("互動式 Treemap 空間佔比矩形圖（點擊子資料夾可下鑽分析）")}
                        </h3>
                        {treeSizeParentPath && (
                          <button
                            type="button"
                            onClick={handleTreeSizeGoParent}
                            className="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 border border-slate-700 hover:border-slate-600 transition-colors cursor-pointer"
                            title={t("回上一層目錄")}
                          >
                            <ArrowUp className="w-3.5 h-3.5" />
                            <span>{t("回上一層")}</span>
                          </button>
                        )}
                      </div>
                      {treeSizeReport.capabilities.is_network_drive && (
                        <span className="text-xs px-2.5 py-0.5 rounded-full bg-amber-500/20 text-amber-300 border border-amber-500/40">
                          {t("🌐 網路磁碟受控掃描 (最大深度 {depth} 層)", {
                            depth: treeSizeReport.capabilities.max_scan_depth,
                          })}
                        </span>
                      )}
                    </div>

                    <div className="flex flex-wrap gap-2 min-h-[180px] p-2 rounded-xl bg-slate-950 border border-slate-800">
                      {treeSizeReport.children.slice(0, 12).map((node, i) => {
                        const flexBasis = Math.max(
                          12,
                          Math.min(100, node.percentage)
                        );
                        const colorClass =
                          TREEMAP_COLORS[i % TREEMAP_COLORS.length];
                        return (
                          <button
                            key={node.path}
                            onClick={() => {
                              if (node.is_dir) {
                                handleTreeSizeDrillDown(node.path);
                              }
                            }}
                            style={{ flexGrow: Math.round(flexBasis), flexBasis: `${flexBasis}%` }}
                            className={`p-3 rounded-xl border text-left transition-transform ${node.is_dir ? 'hover:scale-[1.01] cursor-pointer' : 'cursor-default opacity-95'} flex flex-col justify-between min-w-[140px] min-h-[84px] ${colorClass}`}
                          >
                            <div className="flex items-center justify-between gap-1">
                              <span className="font-bold text-xs text-white truncate">
                                {node.is_dir ? '📁 ' : '📄 '}
                                {node.name}
                              </span>
                              <span className="text-[11px] font-mono bg-slate-950/50 px-1.5 py-0.5 rounded text-white">
                                {node.percentage.toFixed(1)}%
                              </span>
                            </div>
                            <div className="text-xs font-mono text-white/90 mt-2">
                              {formatBytes(node.size_bytes)} ({t("{n} 檔", { n: node.file_count })})
                            </div>
                          </button>
                        );
                      })}
                    </div>
                  </div>

                  {/* 左右雙欄：檔案類型統計 + Top 巨型檔案 */}
                  <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
                    <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                      <h3 className="text-sm font-semibold text-white">
                        {t("依檔案類型空間分布")}
                      </h3>
                      <div className="space-y-2.5 text-xs">
                        {treeSizeReport.category_breakdown.map((cat) => (
                          <div key={cat.category} className="space-y-1">
                            <div className="flex justify-between">
                              <span className="text-slate-200 font-medium">
                                {tx(cat.label)} ({t("{n} 檔", { n: cat.file_count })})
                              </span>
                              <span className="font-mono text-slate-300">
                                {formatBytes(cat.size_bytes)} (
                                {cat.percentage.toFixed(1)}%)
                              </span>
                            </div>
                            <div className="w-full h-2 bg-slate-800 rounded-full overflow-hidden">
                              <div
                                className="h-full bg-cyan-400 rounded-full"
                                style={{
                                  width: `${Math.max(2, cat.percentage)}%`,
                                }}
                              />
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>

                    <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                      <h3 className="text-sm font-semibold text-white">
                        {t("Top 20 巨型檔案排行榜")}
                      </h3>
                      <div className="space-y-1.5 max-h-72 overflow-y-auto text-xs">
                        {treeSizeReport.largest_files.map((f) => (
                          <div
                            key={f.path}
                            className="flex items-center justify-between p-2 rounded-lg bg-slate-950/70 border border-slate-800/80"
                          >
                            <div className="truncate pr-3">
                              <div className="font-medium text-slate-200 truncate">
                                {f.name}
                              </div>
                              <div className="text-[11px] text-slate-500 truncate">
                                {f.path}
                              </div>
                            </div>
                            <div className="flex items-center gap-2 shrink-0">
                              {/\.vhdx?$/i.test(f.path) && (
                                <button
                                  type="button"
                                  disabled={vhdxJob?.state === 'running'}
                                  onClick={() => handleCompactVhdx(f.path, f.name)}
                                  title={t("壓縮虛擬磁碟（需要系統管理員權限）")}
                                  className="flex items-center gap-1 px-2 py-1 rounded-lg bg-cyan-500/15 border border-cyan-500/40 text-cyan-200 hover:bg-cyan-500/25 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
                                >
                                  <Minimize2 className="w-3.5 h-3.5" />
                                  <span>
                                    {vhdxJob?.state === 'running' && vhdxJob.path === f.path
                                      ? t("壓縮中...")
                                      : t("壓縮")}
                                  </span>
                                </button>
                              )}
                              <span className="font-mono text-cyan-300">
                                {formatBytes(f.size_bytes)}
                              </span>
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </>
              )}

              {/* 尚未掃描或切換目錄後的狀態提示 */}
              {!treeSizeLoading && !treeSizeReport && (
                <div className="p-12 text-center text-slate-400 text-xs bg-slate-900/40 rounded-2xl border border-slate-800/60 flex flex-col items-center justify-center space-y-3">
                  <div className="w-12 h-12 rounded-2xl bg-slate-950 border border-slate-800 flex items-center justify-center text-slate-500">
                    <PieChart className="w-6 h-6" />
                  </div>
                  <div className="max-w-md space-y-1">
                    <p className="text-slate-300 font-medium">
                      {treeSizePath
                        ? t("目錄已切換。點擊上方「立即掃描分析」按鈕以分析空間佔比與巨型檔案。")
                        : t("請輸入或選擇目錄路徑，並點擊「立即掃描分析」以檢視空間佔比與巨型檔案")}
                    </p>
                    {treeSizePath && (
                      <p className="text-[11px] font-mono text-slate-500 break-all">
                        {treeSizePath}
                      </p>
                    )}
                  </div>
                  {treeSizePath && (
                    <button
                      type="button"
                      onClick={() => runTreeSize(treeSizePath, true)}
                      className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5 cursor-pointer transition-colors shadow-md shadow-cyan-500/10"
                    >
                      <PieChart className="w-4 h-4" />
                      <span>{t("立即掃描分析")}</span>
                    </button>
                  )}
                </div>
              )}
            </div>
          )}

          {/* ==================== 3. 工作管理員 & Port Hunter ==================== */}
          {(
            <div className={activeTab === 'taskmgr' ? "flex-1 overflow-y-auto p-5 space-y-5" : "hidden"}>
              {/* 硬體即時儀表板 */}
              {sysSnapshot && (
                <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>{t("CPU 總負載")}</span>
                      <Cpu className="w-4 h-4 text-cyan-400" />
                    </div>
                    <div className="text-2xl font-bold text-white">
                      {sysSnapshot.global_cpu_usage.toFixed(1)}%
                    </div>
                    <div className="text-[11px] text-slate-400 truncate mt-1">
                      {sysSnapshot.cpu_brand}
                    </div>
                  </div>

                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>{t("實體記憶體 (RAM)")}</span>
                      <Database className="w-4 h-4 text-indigo-400" />
                    </div>
                    <div className="text-2xl font-bold text-white">
                      {(
                        (sysSnapshot.used_memory_bytes /
                          Math.max(1, sysSnapshot.total_memory_bytes)) *
                        100
                      ).toFixed(1)}
                      %
                    </div>
                    <div className="text-[11px] text-slate-400 mt-1">
                      {formatBytes(sysSnapshot.used_memory_bytes)} /{' '}
                      {formatBytes(sysSnapshot.total_memory_bytes)}
                    </div>
                  </div>

                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>{t("活躍處理程序")}</span>
                      <Activity className="w-4 h-4 text-emerald-400" />
                    </div>
                    <div className="text-2xl font-bold text-white">
                      {sysSnapshot.total_processes}
                    </div>
                    <div className="text-[11px] text-slate-400 mt-1">
                      {t("監聽連接埠：{n} 個", { n: ports.length })}
                    </div>
                  </div>

                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>{t("系統連續運作時間")}</span>
                      <RefreshCw className="w-4 h-4 text-amber-400" />
                    </div>
                    <div className="text-lg font-bold text-white">
                      {formatUptime(sysSnapshot.uptime_secs, t)}
                    </div>
                    <div className="text-[11px] text-slate-400 truncate mt-1">
                      {sysSnapshot.host_name} · {sysSnapshot.os_version}
                    </div>
                  </div>
                </div>
              )}

              {/* 子分頁切換與搜尋 */}
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex gap-2">
                  <button
                    onClick={() => setTaskSubTab('processes')}
                    className={`px-4 py-2 rounded-xl text-xs font-semibold ${
                      taskSubTab === 'processes'
                        ? 'bg-cyan-500 text-slate-950'
                        : 'bg-slate-900 text-slate-300 border border-slate-800'
                    }`}
                  >
                    {t("處理程序清單 (Processes)")}
                  </button>
                  <button
                    onClick={() => setTaskSubTab('ports')}
                    className={`px-4 py-2 rounded-xl text-xs font-semibold ${
                      taskSubTab === 'ports'
                        ? 'bg-cyan-500 text-slate-950'
                        : 'bg-slate-900 text-slate-300 border border-slate-800'
                    }`}
                  >
                    {t("🔌 Port 連接埠佔用獵人 ({n})", { n: ports.length })}
                  </button>
                </div>

                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={taskSearch}
                    onChange={(e) => setTaskSearch(e.target.value)}
                    placeholder={t("搜尋 Process 名稱、PID 或 Port (如 8080)...")}
                    className="bg-slate-900 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100 w-72"
                  />
                  <button
                    onClick={refreshTaskManager}
                    disabled={loading}
                    className="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 flex items-center gap-1.5 disabled:opacity-40"
                  >
                    <RefreshCw className="w-3.5 h-3.5" />
                    {t("重新整理")}
                  </button>
                </div>
              </div>

              {taskSubTab === 'processes' ? (
                <ProcessTable snapshot={sysSnapshot} search={taskSearch}
                  loading={loading} onKill={handleKillProcess} t={t} />
              ) : (
                <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                        <th className="py-2.5 px-3">{t("協定")}</th>
                        <th className="py-2.5 px-3">{t("本機 Port")}</th>
                        <th className="py-2.5 px-3">{t("本機位址")}</th>
                        <th className="py-2.5 px-3">{t("狀態")}</th>
                        <th className="py-2.5 px-3">{t("PID & 佔用程式")}</th>
                        <th className="py-2.5 px-3 text-right">{t("釋放連接埠")}</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800/60">
                      {ports
                        .filter(
                          (pt) =>
                            !taskSearch.trim() ||
                            String(pt.local_port).includes(taskSearch.trim()) ||
                            pt.process_name
                              .toLowerCase()
                              .includes(taskSearch.toLowerCase()) ||
                            String(pt.pid).includes(taskSearch.trim())
                        )
                        .map((pt, i) => (
                          <tr
                            key={`${pt.protocol}-${pt.local_port}-${pt.pid}-${i}`}
                            className="hover:bg-slate-800/50"
                          >
                            <td className="py-2 px-3 font-mono">
                              {pt.protocol}
                            </td>
                            <td className="py-2 px-3 font-mono font-bold text-cyan-300">
                              :{pt.local_port}
                            </td>
                            <td className="py-2 px-3 font-mono text-slate-300">
                              {pt.local_address}
                            </td>
                            <td className="py-2 px-3 text-emerald-300">
                              {pt.state}
                            </td>
                            <td className="py-2 px-3">
                              <span className="font-semibold text-white">
                                {pt.process_name}
                              </span>{' '}
                              <span className="text-slate-400 font-mono">
                                (PID: {pt.pid})
                              </span>
                            </td>
                            <td className="py-2 px-3 text-right">
                              {pt.pid > 0 && (
                                <button
                                  onClick={() => handleKillProcess(pt.pid)}
                                  className="px-2.5 py-1 rounded bg-rose-500/20 hover:bg-rose-500/40 text-rose-300"
                                >
                                  {t("釋放 Port (Kill)")}
                                </button>
                              )}
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          {/* ==================== 4. FreeFileSync 智慧檔案同步 ==================== */}
          {(
            <div className={activeTab === 'sync' ? "flex-1 overflow-y-auto p-5 space-y-5" : "hidden"}>
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-4 space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h2 className="text-sm font-semibold text-white flex items-center gap-2">
                    <FolderSync className="w-4 h-4 text-cyan-400" />
                    {t("FreeFileSync 雙目錄差異比對與智慧同步（內建網路磁碟 2 秒 SMB 時間容差）")}
                  </h2>

                  <div className="flex flex-wrap items-center gap-2 text-xs">
                    <select
                      value={syncCompareMode}
                      onChange={(e) =>
                        setSyncCompareMode(e.target.value as CompareMode)
                      }
                      className="bg-slate-950 border border-slate-800 rounded-xl px-3 py-1.5 text-slate-200"
                    >
                      <option value="time_and_size">
                        {t("比對模式：修改時間 + 檔案大小 (極速)")}
                      </option>
                      <option value="blake3_hash">
                        {t("比對模式：BLAKE3 內容雜湊 (精確驗證)")}
                      </option>
                    </select>

                    <select
                      value={syncStrategy}
                      onChange={(e) =>
                        setSyncStrategy(e.target.value as SyncStrategy)
                      }
                      className="bg-slate-950 border border-slate-800 rounded-xl px-3 py-1.5 text-slate-200"
                    >
                      <option value="mirror">
                        {t("同步策略：鏡像 (Mirror 左 → 右)")}
                      </option>
                      <option value="update">
                        {t("同步策略：更新 (Update 左 → 右，不刪除)")}
                      </option>
                      <option value="two_way">
                        {t("同步策略：雙向互補 (Two-Way)")}
                      </option>
                    </select>
                  </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">
                      {t("左側來源目錄 (Source / Left)")}
                    </label>
                    <input
                      type="text"
                      value={syncLeftPath}
                      onFocus={() => setSyncDriveSide('left')}
                      onChange={(e) => updateSyncPath(e.target.value, 'left')}
                      className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100"
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">
                      {t("右側目標目錄 (Target / Right — 支援本機或 \\\\NAS\\Backup)")}
                    </label>
                    <input
                      type="text"
                      value={syncRightPath}
                      onFocus={() => setSyncDriveSide('right')}
                      onChange={(e) => updateSyncPath(e.target.value, 'right')}
                      className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100"
                    />
                  </div>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  <button
                    onClick={runSyncCompare}
                    className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <ArrowLeftRight className="w-4 h-4" />
                    {t("1. 比對左右目錄差異")}
                  </button>
                  <button
                    onClick={() => runSyncExecute(true)}
                    disabled={!syncReport}
                    className="px-4 py-2 rounded-xl bg-indigo-500/20 border border-indigo-500/40 text-indigo-200 hover:bg-indigo-500/30 disabled:opacity-40 text-xs font-medium"
                  >
                    {t("2. 預演同步 (Dry-Run)")}
                  </button>
                  <button
                    onClick={() => runSyncExecute(false)}
                    disabled={!syncReport}
                    className="px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-semibold disabled:opacity-40 text-xs"
                  >
                    {t("3. 立即執行同步")}
                  </button>
                </div>
              </div>

              {syncExecResult && (
                <div className="p-4 rounded-2xl bg-emerald-950/50 border border-emerald-700/60 text-xs text-emerald-200 flex flex-wrap items-center justify-between gap-4">
                  <div>
                    <strong>
                      {syncExecResult.dry_run ? t("預演結果：") : t("同步完成：")}
                    </strong>{' '}
                    {t("左→右複製 {l2r} 檔 · 右→左複製 {r2l} 檔 · 右側清理 {del} 檔（傳輸總量 {size}）", {
                      l2r: syncExecResult.copied_left_to_right,
                      r2l: syncExecResult.copied_right_to_left,
                      del: syncExecResult.deleted_right,
                      size: formatBytes(syncExecResult.bytes_copied),
                    })}
                  </div>
                </div>
              )}

              {syncReport && (
                <div className="space-y-4">
                  {syncReport.network_advisory && (
                    <div className="p-3 rounded-xl bg-amber-950/60 border border-amber-700/60 text-xs text-amber-200">
                      {tx(syncReport.network_advisory)}
                    </div>
                  )}

                  <div className="grid grid-cols-2 md:grid-cols-6 gap-3 text-xs">
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("僅左側存在")}</div>
                      <div className="text-lg font-bold text-cyan-300">
                        {syncReport.only_left_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("僅右側存在")}</div>
                      <div className="text-lg font-bold text-amber-300">
                        {syncReport.only_right_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("左側較新")}</div>
                      <div className="text-lg font-bold text-emerald-300">
                        {syncReport.left_newer_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("右側較新")}</div>
                      <div className="text-lg font-bold text-indigo-300">
                        {syncReport.right_newer_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("內容不同")}</div>
                      <div className="text-lg font-bold text-rose-300">
                        {syncReport.different_content_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">{t("完全相同")}</div>
                      <div className="text-lg font-bold text-slate-300">
                        {syncReport.identical_count}
                      </div>
                    </div>
                  </div>

                  <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                    <table className="w-full text-left text-xs">
                      <thead>
                        <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                          <th className="py-2.5 px-3">{t("相對路徑")}</th>
                          <th className="py-2.5 px-3 text-right">{t("左側大小")}</th>
                          <th className="py-2.5 px-3 text-center">{t("差異狀態")}</th>
                          <th className="py-2.5 px-3 text-right">{t("右側大小")}</th>
                          <th className="py-2.5 px-3 text-right">{t("預定同步動作")}</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-slate-800/60">
                        {syncReport.items.slice(0, 150).map((item) => (
                          <tr
                            key={item.relative_path}
                            className="hover:bg-slate-800/50"
                          >
                            <td className="py-2 px-3 font-mono text-slate-200">
                              {item.relative_path}
                            </td>
                            <td className="py-2 px-3 text-right font-mono text-slate-300">
                              {item.left_size !== null
                                ? formatBytes(item.left_size)
                                : '-'}
                            </td>
                            <td className="py-2 px-3 text-center">
                              <span className="px-2 py-0.5 rounded bg-slate-800 text-cyan-300">
                                {item.status}
                              </span>
                            </td>
                            <td className="py-2 px-3 text-right font-mono text-slate-300">
                              {item.right_size !== null
                                ? formatBytes(item.right_size)
                                : '-'}
                            </td>
                            <td className="py-2 px-3 text-right font-semibold text-emerald-300">
                              {item.proposed_action}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </div>
              )}
            </div>
          )}

          {/* ==================== 5. 電腦本體維運中心 (System Ops) ==================== */}
          {(
            <div className={activeTab === 'ops' ? "flex-1 overflow-y-auto p-5 space-y-5" : "hidden"}>
              {/* 開發垃圾與重複檔案/圖片掃描列 */}
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-4 space-y-3">
                <h2 className="text-sm font-semibold text-white flex items-center gap-2">
                  <Sparkles className="w-4 h-4 text-cyan-400" />
                  {t("開發者建置快取清理 (`target` / `node_modules` / `.venv`) & 重複檔案/重複圖片查殺")}
                </h2>
                <div className="flex flex-wrap gap-2">
                  <input
                    type="text"
                    value={opsTargetPath}
                    onChange={(e) => updateOpsPath(e.target.value)}
                    placeholder={t("輸入要掃描的工作區目錄路徑...")}
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100 min-w-[260px]"
                  />
                  <button
                    onClick={runJunkScan}
                    className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <Trash2 className="w-4 h-4" />
                    {t("掃描開發建置垃圾")}
                  </button>
                  <button
                    onClick={runDuplicateScan}
                    className="px-4 py-2 rounded-xl bg-pink-500 hover:bg-pink-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <ImageIcon className="w-4 h-4" />
                    {t("掃描重複檔案与圖片 (BLAKE3)")}
                  </button>
                </div>
              </div>

              {/* 開發建置垃圾結果 */}
              {junkReport && (
                <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-white">
                      {t("發現可回收的開發者快取目錄（共計")}{' '}
                      <span className="text-cyan-300">
                        {formatBytes(junkReport.total_reclaimable_bytes)}
                      </span>
                      {t("）")}
                    </h3>
                  </div>
                  {junkReport.items.length === 0 ? (
                    <p className="text-xs text-slate-400">
                      {t("此目錄下未發現大型開發快取目錄。")}
                    </p>
                  ) : (
                    <div className="space-y-2 text-xs">
                      {junkReport.items.map((item) => (
                        <div
                          key={item.path}
                          className="flex items-center justify-between p-3 rounded-xl bg-slate-950 border border-slate-800"
                        >
                          <div>
                            <div className="font-semibold text-white">
                              {item.kind} — {tx(item.description)}
                            </div>
                            <div className="text-slate-400 font-mono">
                              {item.path} ({t("{n} 個檔案", { n: item.file_count })})
                            </div>
                          </div>
                          <span className="font-mono font-bold text-amber-300">
                            {formatBytes(item.size_bytes)}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {/* 重複檔案與重複圖片結果 */}
              {dupReport && (
                <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                  <h3 className="text-sm font-semibold text-white">
                    {t("BLAKE3 重複檔案與重複圖片報表（共 {n} 組，可省下", { n: dupReport.total_groups })}{' '}
                    <span className="text-pink-300">
                      {formatBytes(dupReport.total_wasted_bytes)}
                    </span>
                    {t("）")}
                  </h3>
                  <div className="space-y-2.5 text-xs">
                    {dupReport.groups.map((g) => (
                      <div
                        key={g.blake3_hash}
                        className="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-1.5"
                      >
                        <div className="flex items-center justify-between">
                          <span className="font-mono text-cyan-300">
                            BLAKE3: {g.blake3_hash}...{' '}
                            {g.is_image && t("🖼️ (重複圖片)")}
                          </span>
                          <span className="text-amber-300 font-semibold">
                            {t("單檔 {size} · 浪費 {wasted}", {
                              size: formatBytes(g.file_size_bytes),
                              wasted: formatBytes(g.wasted_bytes),
                            })}
                          </span>
                        </div>
                        <ul className="list-disc list-inside text-slate-300 space-y-0.5">
                          {g.files.map((f) => (
                            <li key={f} className="font-mono truncate">
                              {f}
                            </li>
                          ))}
                        </ul>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* 系統磁碟健康度、開機啟動項與 PATH 環境變數體檢 */}
              {maintOverview && (
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-5">
                  <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                    <h3 className="text-sm font-semibold text-white">
                      {t("💽 磁碟健康度 (S.M.A.R.T.) 與系統暫存區狀態")}
                    </h3>
                    <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs flex justify-between">
                      <span className="text-slate-400">
                        {t("Windows Temp 暫存目錄 ({path})：", { path: maintOverview.temp_dir_path })}
                      </span>
                      <span className="font-mono font-bold text-amber-300">
                        {formatBytes(maintOverview.temp_dir_size_bytes)}
                      </span>
                    </div>
                    <div className="space-y-2 text-xs">
                      {maintOverview.disk_health_summaries.map((d) => (
                        <div
                          key={d.mount_point}
                          className="p-3 rounded-xl bg-slate-950 border border-slate-800 space-y-1"
                        >
                          <div className="flex items-center justify-between">
                            <span className="font-bold text-white">
                              {d.mount_point} ({tx(d.name)})
                            </span>
                            <span className="text-xs">{tx(d.health_badge)}</span>
                          </div>
                          <div className="text-slate-400">{tx(d.note)}</div>
                        </div>
                      ))}
                    </div>

                    <h4 className="text-xs font-semibold text-slate-300 pt-2">
                      {t("🚀 偵測到的使用者開機啟動項 ({n})", { n: maintOverview.startup_entries.length })}
                    </h4>
                    <div className="space-y-1.5 text-xs">
                      {maintOverview.startup_entries.map((st) => (
                        <div
                          key={st.command_or_path}
                          className="p-2.5 rounded-lg bg-slate-950 border border-slate-800"
                        >
                          <div className="font-semibold text-slate-200">
                            {st.name}
                          </div>
                          <div className="text-[11px] text-slate-500 truncate">
                            {st.command_or_path}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>

                  <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-white">
                        {t("🩺 系統 PATH 環境變數體檢")}
                      </h3>
                      <span className="text-xs text-amber-300">
                        {t("失效路徑：{broken} 筆 · 網路路徑：{net} 筆", {
                          broken: maintOverview.broken_path_count,
                          net: maintOverview.network_path_count,
                        })}
                      </span>
                    </div>
                    <div className="space-y-1.5 max-h-96 overflow-y-auto text-xs">
                      {maintOverview.path_diagnostics.map((pd) => (
                        <div
                          key={`${pd.index}-${pd.path}`}
                          className={`p-2.5 rounded-xl border ${
                            pd.status === 'missing_directory'
                              ? 'bg-rose-950/40 border-rose-800/70 text-rose-200'
                              : pd.status === 'network_path_warning'
                                ? 'bg-amber-950/40 border-amber-800/70 text-amber-200'
                                : 'bg-slate-950 border-slate-800 text-slate-300'
                          }`}
                        >
                          <div className="font-mono truncate">
                            #{pd.index} {pd.path}
                          </div>
                          <div className="text-[11px] opacity-80 mt-0.5">
                            {tx(pd.note)}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}
        </main>
      </div>

      {vhdxTarget && (
        <VhdxCompactDialog
          path={vhdxTarget.path} name={vhdxTarget.name} job={vhdxJob}
          t={t} tx={tx} onJob={setVhdxJob} onClose={() => setVhdxTarget(null)}
        />
      )}
      {vhdxPollError && (
        <div role="alert" className="fixed bottom-4 left-4 right-4 z-[70] rounded-xl bg-rose-950 p-3 text-sm text-rose-200">
          {t("無法取得壓縮狀態，將自動重試；請勿關閉程式或啟動 WSL／Docker。")} {tx(vhdxPollError)}
        </div>
      )}
      {/* 檔案總管就地 LockHunter 佔用解鎖 Modal */}
      {lockModalOpen && (
        <div
          className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setLockModalOpen(false)}
        >
          <div
            className="w-full max-w-2xl bg-slate-900 border border-slate-700 rounded-2xl shadow-2xl flex flex-col max-h-[85vh] overflow-hidden animate-in fade-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal 標題列 */}
            <div className="p-4 border-b border-slate-800 flex items-center justify-between gap-3 bg-slate-900/90 shrink-0">
              <div className="flex items-center gap-2.5 min-w-0">
                <div className="w-8 h-8 rounded-lg bg-amber-500/20 border border-amber-500/30 flex items-center justify-center text-amber-400 shrink-0">
                  <Unlock className="w-4 h-4" />
                </div>
                <div className="min-w-0">
                  <h3 className="font-semibold text-sm text-white flex items-center gap-2">
                    <span>{t("LockHunter 解鎖")}</span>
                    <span className="text-[11px] px-2 py-0.5 rounded-full bg-slate-800 text-slate-300 font-normal">
                      Modal
                    </span>
                  </h3>
                  <div
                    className="text-[11px] text-slate-400 font-mono truncate select-text cursor-text"
                    title={lockModalPath}
                  >
                    {lockModalPath}
                  </div>
                </div>
              </div>

              <div className="flex items-center gap-1.5 shrink-0">
                <button
                  onClick={refreshLockModal}
                  disabled={lockModalLoading}
                  className="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-300 disabled:opacity-40 transition-colors"
                  title={t("重新整理")}
                >
                  <RefreshCw
                    className={`w-4 h-4 ${lockModalLoading ? 'animate-spin text-cyan-400' : ''}`}
                  />
                </button>
                <button
                  onClick={() => setLockModalOpen(false)}
                  className="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors"
                  title={t("關閉")}
                >
                  <X className="w-4 h-4" />
                </button>
              </div>
            </div>

            {/* Modal 內容區 */}
            <div className="flex-1 overflow-y-auto p-4 space-y-4">
              {lockModalLoading && !lockModalReport && (
                <div className="py-12 flex flex-col items-center justify-center text-slate-400 space-y-3">
                  <RefreshCw className="w-7 h-7 text-amber-400 animate-spin" />
                  <span className="text-xs">
                    {t("Rust 引擎讀取中...")}
                  </span>
                </div>
              )}

              {lockModalReport && (
                <>
                  {/* 鎖定狀態卡片 */}
                  <div
                    className={`p-3.5 rounded-xl border flex items-center justify-between gap-3 ${
                      lockModalReport.is_locked
                        ? 'bg-rose-950/30 border-rose-800/60 text-rose-200'
                        : 'bg-emerald-950/30 border-emerald-800/60 text-emerald-200'
                    }`}
                  >
                    <div className="flex items-center gap-2.5">
                      {lockModalReport.is_locked ? (
                        <Lock className="w-5 h-5 text-rose-400 shrink-0" />
                      ) : (
                        <Unlock className="w-5 h-5 text-emerald-400 shrink-0" />
                      )}
                      <div>
                        <div className="font-semibold text-xs text-white">
                          {lockModalReport.is_locked
                            ? t("發現 {n} 個處理程序正在佔用此路徑", {
                                n: lockModalReport.locking_processes.length,
                              })
                            : t("目前未偵測到任何本機行程鎖定此路徑，可安全操作")}
                        </div>
                        <div className="text-[11px] opacity-75 mt-0.5">
                          {t("已檢查檔案數：{n} 個", {
                            n: lockModalReport.scanned_files_count,
                          })}
                        </div>
                      </div>
                    </div>

                    <span
                      className={`text-[11px] px-2.5 py-1 rounded-full font-medium shrink-0 ${
                        lockModalReport.scope === 'local_process_only'
                          ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40'
                          : 'bg-emerald-500/20 text-emerald-300'
                      }`}
                    >
                      {lockModalReport.scope === 'local_process_only'
                        ? t("🌐 網路磁碟：僅限本機行程範圍")
                        : t("💻 本機完整支援")}
                    </span>
                  </div>

                  {/* 診斷小叮嚀 */}
                  {lockModalReport.advisory_notes.length > 0 && (
                    <div className="space-y-1 text-xs text-slate-400 bg-slate-950/60 p-3 rounded-xl border border-slate-800/80">
                      {lockModalReport.advisory_notes.map((n, idx) => (
                        <div key={idx} className="leading-relaxed">
                          {tx(n)}
                        </div>
                      ))}
                    </div>
                  )}

                  {/* 佔用行程表格 */}
                  {lockModalReport.locking_processes.length > 0 ? (
                    <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-950">
                      <table className="w-full text-left text-xs">
                        <thead>
                          <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                            <th className="py-2 px-3">PID</th>
                            <th className="py-2 px-3">{t("行程名稱")}</th>
                            <th className="py-2 px-3">{t("類型")}</th>
                            <th className="py-2 px-3">{t("鎖定原因")}</th>
                            <th className="py-2 px-3 text-right">
                              {t("解鎖動作")}
                            </th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-slate-800/60">
                          {lockModalReport.locking_processes.map((lp) => (
                            <tr
                              key={lp.pid}
                              className="hover:bg-slate-900/60 transition-colors"
                            >
                              <td className="py-2.5 px-3 font-mono text-amber-300">
                                {lp.pid}
                              </td>
                              <td className="py-2.5 px-3 font-semibold text-white">
                                {lp.process_name}
                              </td>
                              <td className="py-2.5 px-3 text-slate-300">
                                {lp.app_type}
                              </td>
                              <td className="py-2.5 px-3 text-slate-300">
                                {tx(lp.lock_reason)}
                              </td>
                              <td className="py-2.5 px-3 text-right">
                                {lp.pid === 4 ? (
                                  <button
                                    disabled={ejectingDrive !== null}
                                    onClick={() =>
                                      handleSafeEjectDrive(lockModalReport.target_path)
                                    }
                                    className="px-2.5 py-1 rounded-lg bg-amber-500 hover:bg-amber-400 text-slate-950 font-semibold text-xs shadow-sm transition-colors flex items-center gap-1 ml-auto"
                                  >
                                    <Disc className={`w-3.5 h-3.5 ${ejectingDrive ? 'animate-spin' : ''}`} />
                                    <span>{t("安全卸載並彈出")}</span>
                                  </button>
                                ) : (
                                  <button
                                    onClick={() =>
                                      handleKillLockProcess(lp.pid)
                                    }
                                    className="px-2.5 py-1 rounded-lg bg-rose-500 hover:bg-rose-400 text-white font-semibold text-xs shadow-sm transition-colors"
                                  >
                                    {t("強制終止並解鎖")}
                                  </button>
                                )}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : null}

                  {/* 下次開機強制刪除排程 */}
                  {lockModalReport.capabilities.supports_boot_delete && (
                    <div className="pt-2 flex justify-end">
                      <button
                        onClick={async () => {
                          try {
                            const msg = await keeperApi.scheduleBootDelete(
                              lockModalReport.target_path
                            );
                            notifyStatus(msg);
                          } catch (err) {
                            notifyError(err);
                          }
                        }}
                        className="px-3 py-1.5 rounded-xl bg-rose-500/20 border border-rose-500/40 text-rose-200 hover:bg-rose-500/30 text-xs"
                      >
                        {t("排程於下次重開機時強制刪除")}
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>

            {/* Modal 底端動作列 */}
            <div className="p-3 border-t border-slate-800 bg-slate-900/90 flex items-center justify-between shrink-0">
              <div>
                {isDriveTarget(lockModalReport?.target_path) && (
                  <button
                    disabled={ejectingDrive !== null}
                    onClick={() => handleSafeEjectDrive(lockModalReport!.target_path)}
                    className="px-3 py-1.5 rounded-xl bg-amber-500 hover:bg-amber-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5 shadow-sm transition-colors"
                  >
                    <Disc className={`w-3.5 h-3.5 ${ejectingDrive ? 'animate-spin' : ''}`} />
                    <span>{ejectingDrive ? t("正在安全卸載...") : t("⏏️ 安全彈出外接硬碟 (Safe Eject)")}</span>
                  </button>
                )}
              </div>
              <button
                onClick={() => setLockModalOpen(false)}
                className="px-4 py-1.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 font-medium transition-colors"
              >
                {t("關閉")}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 外接硬碟安全彈出與卸載報告 Modal */}
      {safeEjectModalReport && (
        <div
          className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
          onClick={() => setSafeEjectModalReport(null)}
        >
          <div
            className="w-full max-w-xl bg-slate-900 border border-slate-700 rounded-2xl shadow-2xl flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 標題列 */}
            <div className="p-4 border-b border-slate-800 flex items-center justify-between gap-3 bg-slate-900/90 shrink-0">
              <div className="flex items-center gap-2.5">
                <div
                  className={`w-8 h-8 rounded-lg flex items-center justify-center shrink-0 ${
                    safeEjectModalReport.safe_to_unplug
                      ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/30'
                      : 'bg-amber-500/20 text-amber-400 border border-amber-500/30'
                  }`}
                >
                  <ShieldCheck className="w-5 h-5" />
                </div>
                <div>
                  <h3 className="font-semibold text-sm text-white">
                    {t("外接硬碟安全彈出與卸載報告")}
                  </h3>
                  <div className="text-[11px] text-slate-400 font-mono">
                    {t("磁碟代號：{letter}:", {
                      letter: safeEjectModalReport.drive_letter,
                    })}
                  </div>
                </div>
              </div>
              <button
                onClick={() => setSafeEjectModalReport(null)}
                className="p-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* 內容區 */}
            <div className="p-5 space-y-4">
              <div
                className={`p-4 rounded-xl border ${
                  safeEjectModalReport.safe_to_unplug
                    ? 'bg-emerald-950/30 border-emerald-800/60 text-emerald-200'
                    : 'bg-amber-950/30 border-amber-800/60 text-amber-200'
                }`}
              >
                <div className="font-semibold text-sm text-white flex items-center gap-2">
                  <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                  <span>{tx(safeEjectModalReport.message)}</span>
                </div>
              </div>

              {/* 指標卡片 */}
              <div className="grid grid-cols-3 gap-2.5 text-xs">
                <div className="bg-slate-950/60 p-2.5 rounded-xl border border-slate-800">
                  <div className="text-slate-400 text-[11px] mb-1">
                    {t("快取排清 (Flush)")}
                  </div>
                  <div className="font-semibold text-emerald-400 flex items-center gap-1">
                    <Check className="w-3.5 h-3.5" />
                    <span>
                      {safeEjectModalReport.flush_succeeded
                        ? t("成功落盤")
                        : t("已排清")}
                    </span>
                  </div>
                </div>
                <div className="bg-slate-950/60 p-2.5 rounded-xl border border-slate-800">
                  <div className="text-slate-400 text-[11px] mb-1">
                    {t("卷冊卸載 (Dismount)")}
                  </div>
                  <div className="font-semibold text-emerald-400 flex items-center gap-1">
                    <Check className="w-3.5 h-3.5" />
                    <span>
                      {safeEjectModalReport.dismount_succeeded
                        ? t("成功中斷")
                        : t("已卸載")}
                    </span>
                  </div>
                </div>
                <div className="bg-slate-950/60 p-2.5 rounded-xl border border-slate-800">
                  <div className="text-slate-400 text-[11px] mb-1">
                    {t("開機保護分區")}
                  </div>
                  <div
                    className={`font-semibold flex items-center gap-1 ${
                      safeEjectModalReport.has_system_partition
                        ? 'text-amber-400'
                        : 'text-slate-300'
                    }`}
                  >
                    <span>
                      {safeEjectModalReport.has_system_partition
                        ? t("含有開機分區")
                        : t("無開機分區")}
                    </span>
                  </div>
                </div>
              </div>

              {/* 條列式診斷與建議 */}
              {safeEjectModalReport.advisory.length > 0 && (
                <div className="space-y-2 bg-slate-950/80 p-3.5 rounded-xl border border-slate-800 text-xs">
                  <div className="text-slate-300 font-semibold mb-1 flex items-center gap-1.5">
                    <BookOpen className="w-3.5 h-3.5 text-cyan-400" />
                    <span>{t("深度診斷與安心拔除指引：")}</span>
                  </div>
                  {safeEjectModalReport.advisory.map((adv, idx) => (
                    <div
                      key={idx}
                      className="text-slate-400 leading-relaxed flex items-start gap-2"
                    >
                      <span className="text-cyan-400 shrink-0">•</span>
                      <span>{tx(adv)}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {/* 底端按鈕 */}
            <div className="p-3 border-t border-slate-800 bg-slate-900/90 flex justify-end shrink-0">
              <button
                onClick={() => setSafeEjectModalReport(null)}
                className="px-5 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-bold text-xs shadow-md transition-colors"
              >
                {t("已了解，可安心拔除外接硬碟")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
export default App;

import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  ArrowLeftRight,
  ArrowUp,
  CheckCircle2,
  Cpu,
  Database,
  Eye,
  FileCode,
  FileText,
  Folder,
  FolderSync,
  HardDrive,
  Image as ImageIcon,
  Layers,
  Lock,
  Network,
  PieChart,
  RefreshCw,
  Search,
  ShieldAlert,
  Sparkles,
  Trash2,
  Unlock,
  Wrench,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';
import {
  formatBytes,
  formatTimestamp,
  formatUptime,
  keeperApi,
} from './lib/bridge';
import type {
  CompareMode,
  DirectoryListing,
  DriveInfo,
  DuplicateScanReport,
  FileEntry,
  ImagePreviewResult,
  JunkScanReport,
  LockHunterReport,
  PortBindingEntry,
  SyncComparisonReport,
  SyncExecutionSummary,
  SyncStrategy,
  SystemMaintenanceOverview,
  SystemSnapshot,
  TreeSizeReport,
  VolumeCapabilities,
} from './types/keeper';

type ModuleTab =
  | 'explorer'
  | 'treesize'
  | 'taskmgr'
  | 'lockhunter'
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

  // 1. 檔案總管 + 圖片預覽狀態
  const [listing, setListing] = useState<DirectoryListing | null>(null);
  const [searchFilter, setSearchFilter] = useState<string>('');
  const [onlyImagesFilter, setOnlyImagesFilter] = useState<boolean>(false);
  const [selectedEntry, setSelectedEntry] = useState<FileEntry | null>(null);
  const [imagePreview, setImagePreview] = useState<ImagePreviewResult | null>(
    null
  );
  const [imageZoom, setImageZoom] = useState<number>(100);
  const [confirmNetworkPermanentDelete, setConfirmNetworkPermanentDelete] =
    useState<boolean>(false);

  // 2. TreeSize 狀態
  const [treeSizePath, setTreeSizePath] = useState<string>('');
  const [treeSizeReport, setTreeSizeReport] = useState<TreeSizeReport | null>(
    null
  );

  // 3. 工作管理員 + Port Hunter 狀態
  const [sysSnapshot, setSysSnapshot] = useState<SystemSnapshot | null>(null);
  const [ports, setPorts] = useState<PortBindingEntry[]>([]);
  const [taskSubTab, setTaskSubTab] = useState<'processes' | 'ports'>(
    'processes'
  );
  const [taskSearch, setTaskSearch] = useState<string>('');

  // 4. LockHunter 狀態
  const [lockTargetPath, setLockTargetPath] = useState<string>('');
  const [lockReport, setLockReport] = useState<LockHunterReport | null>(null);

  // 5. FreeFileSync 狀態
  const [syncLeftPath, setSyncLeftPath] = useState<string>('');
  const [syncRightPath, setSyncRightPath] = useState<string>('');
  const [syncCompareMode, setSyncCompareMode] =
    useState<CompareMode>('time_and_size');
  const [syncStrategy, setSyncStrategy] = useState<SyncStrategy>('mirror');
  const [syncReport, setSyncReport] = useState<SyncComparisonReport | null>(
    null
  );
  const [syncExecResult, setSyncExecResult] =
    useState<SyncExecutionSummary | null>(null);

  // 6. 電腦本體維運中心 (Ops) 狀態
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
      setLockTargetPath((prev) => prev || data.current_path);
      setSyncLeftPath((prev) => prev || data.current_path);
      setSyncRightPath((prev) => prev || data.current_path);
      setOpsTargetPath((prev) => prev || data.current_path);
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
    loadDirectory();
  }, [loadDirectory]);

  // 當選擇圖片檔案時，自動載入圖片預覽
  const handleSelectEntry = async (entry: FileEntry, forceNetwork = false) => {
    setSelectedEntry(entry);
    if (entry.is_image) {
      try {
        const preview = await keeperApi.previewImage(
          entry.path,
          960,
          forceNetwork
        );
        setImagePreview(preview);
        setImageZoom(100);
      } catch (err) {
        notifyError(err);
      }
    } else {
      setImagePreview(null);
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
      await loadDirectory(currentPath);
    } catch (err) {
      notifyError(err);
    }
  };

  // TreeSize 分析
  const runTreeSize = useCallback(
    async (overridePath?: string) => {
      const target = overridePath ?? treeSizePath ?? currentPath;
      if (!target) return;
      setLoading(true);
      setErrorBanner(null);
      try {
        const report = await keeperApi.analyzeTreeSize(target);
        setTreeSizeReport(report);
        setTreeSizePath(report.root_path);
      } catch (err) {
        notifyError(err);
      } finally {
        setLoading(false);
      }
    },
    [treeSizePath, currentPath]
  );

  // 工作管理員與 Port 監控
  const refreshTaskManager = useCallback(async () => {
    setLoading(true);
    setErrorBanner(null);
    try {
      const [snap, portList] = await Promise.all([
        keeperApi.getSystemSnapshot(100),
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

  // LockHunter 檢查
  const runLockHunter = useCallback(
    async (overridePath?: string) => {
      const target = overridePath ?? lockTargetPath ?? currentPath;
      if (!target) return;
      setLoading(true);
      setErrorBanner(null);
      try {
        const report = await keeperApi.inspectLocks(target);
        setLockReport(report);
        setLockTargetPath(report.target_path);
      } catch (err) {
        notifyError(err);
      } finally {
        setLoading(false);
      }
    },
    [lockTargetPath, currentPath]
  );

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
          ? '已完成 FreeFileSync 同步預演 (Dry-Run)，未實際修改磁碟檔案。'
          : '已完成 FreeFileSync 檔案同步作業！'
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

  // 切換模組時自動載入對應資料
  useEffect(() => {
    if (activeTab === 'treesize' && !treeSizeReport && currentPath) {
      runTreeSize(currentPath);
    } else if (activeTab === 'taskmgr' && !sysSnapshot) {
      refreshTaskManager();
    } else if (activeTab === 'lockhunter' && !lockReport && currentPath) {
      runLockHunter(currentPath);
    } else if (activeTab === 'ops' && !maintOverview) {
      loadSystemOps();
    }
  }, [
    activeTab,
    currentPath,
    treeSizeReport,
    sysSnapshot,
    lockReport,
    maintOverview,
    runTreeSize,
    refreshTaskManager,
    runLockHunter,
    loadSystemOps,
  ]);

  const filteredEntries = useMemo(() => {
    if (!listing) return [];
    return listing.entries.filter((item) => {
      if (onlyImagesFilter && !item.is_dir && !item.is_image) return false;
      if (
        searchFilter.trim() &&
        !item.name.toLowerCase().includes(searchFilter.trim().toLowerCase())
      ) {
        return false;
      }
      return true;
    });
  }, [listing, onlyImagesFilter, searchFilter]);

  const activeCapabilities = listing?.capabilities ?? null;

  return (
    <div className="min-h-screen flex flex-col bg-slate-950 text-slate-100">
      {/* 頂部系統標題與模組導覽列 */}
      <header className="border-b border-slate-800 bg-slate-900/90 backdrop-blur px-5 py-3 flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-cyan-500 to-blue-600 flex items-center justify-center shadow-lg shadow-cyan-500/20">
            <HardDrive className="w-5 h-5 text-white" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="font-bold text-lg tracking-tight text-white">
                My Computer Keeper
              </h1>
              <span className="text-xs px-2 py-0.5 rounded-full bg-cyan-500/15 text-cyan-300 border border-cyan-500/30 font-medium">
                Rust Core + Tauri v2
              </span>
            </div>
            <p className="text-xs text-slate-400">
              檔案總管 · 圖片檢視 · TreeSize · 工作管理員 · LockHunter · FreeFileSync · 電腦本體維運（支援網路磁碟有限功能）
            </p>
          </div>
        </div>

        {/* 六大核心模組導覽 Tabs */}
        <nav className="flex flex-wrap items-center gap-1.5 bg-slate-950/80 p-1.5 rounded-xl border border-slate-800">
          {[
            { id: 'explorer', label: '檔案總管 & 圖片', icon: Folder },
            { id: 'treesize', label: 'TreeSize 空間分析', icon: PieChart },
            { id: 'taskmgr', label: '工作管理員 & Port', icon: Activity },
            { id: 'lockhunter', label: 'LockHunter 解鎖', icon: Unlock },
            { id: 'sync', label: 'FreeFileSync 同步', icon: FolderSync },
            { id: 'ops', label: '電腦本體維運中心', icon: Wrench },
          ].map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id as ModuleTab)}
                className={`flex items-center gap-2 px-3.5 py-2 rounded-lg text-xs font-medium transition-all ${
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
            <span>{errorBanner ?? statusBanner}</span>
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
      {activeCapabilities?.is_network_drive && (
        <div className="bg-amber-950/60 border-b border-amber-700/60 px-5 py-2.5 text-xs text-amber-200 flex flex-col gap-1">
          <div className="flex items-center gap-2 font-semibold text-amber-300">
            <Network className="w-4 h-4" />
            <span>
              🌐 網路磁碟機有限功能模式 (Network Drive Safe Mode) —{' '}
              {activeCapabilities.remote_unc_path ?? currentPath}
            </span>
          </div>
          <ul className="list-disc list-inside text-amber-200/90 space-y-0.5">
            {activeCapabilities.limitations_notice.map((note, idx) => (
              <li key={idx}>{note}</li>
            ))}
          </ul>
        </div>
      )}

      {/* 主內容區 */}
      <div className="flex-1 flex overflow-hidden">
        {/* 左側磁碟機與網路磁碟機狀態欄 */}
        <aside className="w-72 border-r border-slate-800 bg-slate-900/50 p-4 flex flex-col gap-4 overflow-y-auto shrink-0">
          <div>
            <div className="flex items-center justify-between mb-2.5">
              <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                儲存裝置與網路磁碟
              </span>
              <button
                onClick={() => keeperApi.getDrives().then(setDrives)}
                className="text-xs text-cyan-400 hover:text-cyan-300 flex items-center gap-1"
                title="重新整理磁碟清單"
              >
                <RefreshCw className="w-3.5 h-3.5" />
              </button>
            </div>

            <div className="space-y-2.5">
              {drives.map((d) => {
                const isNet = d.capabilities.is_network_drive;
                return (
                  <button
                    key={d.mount_point}
                    onClick={() => {
                      setActiveTab('explorer');
                      loadDirectory(d.mount_point);
                    }}
                    className="w-full text-left p-3 rounded-xl border border-slate-800 bg-slate-900/90 hover:border-cyan-500/50 transition-all group"
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
                        {isNet ? '🌐 網路磁碟 (有限功能)' : d.file_system}
                      </span>
                    </div>
                    <div className="text-xs text-slate-400 truncate mb-2">
                      {d.name}
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
                      <span>已用 {d.usage_percent.toFixed(1)}%</span>
                      <span>可用 {formatBytes(d.available_bytes)}</span>
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
              <span>網路磁碟 (SMB/UNC) 權限檢測</span>
            </div>
            <p className="text-[11px] text-slate-400 mb-2">
              輸入任何本機或遠端 UNC 路徑（如{' '}
              <code className="text-amber-200">\\NAS\Share</code>
              ），即時查驗系統套用的安全降級策略：
            </p>
            <div className="flex gap-1.5 mb-2">
              <input
                type="text"
                value={probeUncInput}
                onChange={(e) => setProbeUncInput(e.target.value)}
                className="flex-1 bg-slate-950 border border-slate-800 rounded-lg px-2.5 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-amber-500"
                placeholder="\\Server\Share 或 C:\"
              />
              <button
                onClick={() => runProbeCapabilities(probeUncInput)}
                className="px-2.5 py-1.5 rounded-lg bg-amber-500/20 text-amber-300 border border-amber-500/40 hover:bg-amber-500/30 text-xs font-medium shrink-0"
              >
                檢測
              </button>
            </div>

            {probedCaps && (
              <div className="p-2.5 rounded-xl bg-slate-950 border border-slate-800 text-[11px] space-y-1.5">
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">識別類型：</span>
                  <span
                    className={`font-semibold ${
                      probedCaps.is_network_drive
                        ? 'text-amber-300'
                        : 'text-emerald-300'
                    }`}
                  >
                    {probedCaps.is_network_drive
                      ? '🌐 網路磁碟 (有限功能)'
                      : '💻 本機磁碟 (完整功能)'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">本機資源回收筒：</span>
                  <span>
                    {probedCaps.supports_recycle_bin ? '✅ 支援' : '⚠️ 不支援 (直接刪除)'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">MFT 極速掃描：</span>
                  <span>
                    {probedCaps.supports_mft_fast_scan
                      ? '✅ 支援'
                      : '⚠️ 停用 (受控執行緒)'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">LockHunter 範圍：</span>
                  <span>
                    {probedCaps.lockhunter_scope === 'full_local'
                      ? '✅ 完整本機行程'
                      : '⚠️ 僅限本機 SMB 句柄'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">圖片自動預覽上限：</span>
                  <span>{formatBytes(probedCaps.max_auto_preview_bytes)}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-slate-400">FreeFileSync 時間容差：</span>
                  <span>{probedCaps.sync_timestamp_tolerance_secs} 秒</span>
                </div>
              </div>
            )}
          </div>
        </aside>

        {/* 右側主工作面板 */}
        <main className="flex-1 flex flex-col overflow-hidden">
          {/* ==================== 1. 檔案總管 & 圖片檢視器 ==================== */}
          {activeTab === 'explorer' && (
            <div className="flex-1 flex flex-col overflow-hidden">
              {/* 路徑導覽與工具列 */}
              <div className="p-3 border-b border-slate-800 bg-slate-900/40 flex flex-wrap items-center gap-2">
                <button
                  onClick={() =>
                    listing?.parent_path && loadDirectory(listing.parent_path)
                  }
                  disabled={!listing?.parent_path}
                  className="p-2 rounded-lg bg-slate-800 hover:bg-slate-700 disabled:opacity-40 text-slate-200"
                  title="回上一層目錄"
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
                    placeholder="輸入本機路徑或 \\Server\Share 網路磁碟路徑..."
                  />
                  <button
                    type="submit"
                    className="px-3 py-1.5 rounded-lg bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs"
                  >
                    前往
                  </button>
                </form>

                {/* 搜尋與圖片篩選 */}
                <div className="relative">
                  <Search className="w-3.5 h-3.5 text-slate-400 absolute left-2.5 top-2" />
                  <input
                    type="text"
                    value={searchFilter}
                    onChange={(e) => setSearchFilter(e.target.value)}
                    placeholder="過濾檔案名稱..."
                    className="bg-slate-950 border border-slate-800 rounded-lg pl-8 pr-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-cyan-500 w-44"
                  />
                </div>

                <button
                  onClick={() => setOnlyImagesFilter((v) => !v)}
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium border ${
                    onlyImagesFilter
                      ? 'bg-pink-500/20 border-pink-500/50 text-pink-200'
                      : 'bg-slate-900 border-slate-800 text-slate-300 hover:bg-slate-800'
                  }`}
                >
                  <ImageIcon className="w-3.5 h-3.5" />
                  僅顯示圖片 ({listing?.total_images ?? 0})
                </button>

                {/* 跨模組快速聯動按鈕 */}
                <button
                  onClick={() => {
                    setTreeSizePath(currentPath);
                    setActiveTab('treesize');
                    runTreeSize(currentPath);
                  }}
                  className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-indigo-500/20 text-indigo-300 border border-indigo-500/30 hover:bg-indigo-500/30 text-xs"
                >
                  <PieChart className="w-3.5 h-3.5" />
                  分析此目錄空間
                </button>

                <button
                  onClick={() => {
                    setLockTargetPath(selectedEntry?.path ?? currentPath);
                    setActiveTab('lockhunter');
                    runLockHunter(selectedEntry?.path ?? currentPath);
                  }}
                  className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg bg-amber-500/20 text-amber-300 border border-amber-500/30 hover:bg-amber-500/30 text-xs"
                >
                  <Unlock className="w-3.5 h-3.5" />
                  檢查誰鎖住了此處
                </button>
              </div>

              {/* 檔案清單 + 右側圖片檢視面板 */}
              <div className="flex-1 flex overflow-hidden">
                {/* 檔案表格 */}
                <div className="flex-1 overflow-y-auto p-3">
                  <div className="mb-2 flex items-center justify-between text-xs text-slate-400 px-2">
                    <span>
                      共 {listing?.total_dirs ?? 0} 個資料夾、
                      {listing?.total_files ?? 0} 個檔案（總計{' '}
                      {formatBytes(listing?.total_size_bytes ?? 0)}）
                    </span>
                    {loading && (
                      <span className="text-cyan-400 animate-pulse">
                        Rust 引擎讀取中...
                      </span>
                    )}
                  </div>

                  <div className="border border-slate-800 rounded-xl overflow-hidden bg-slate-900/40">
                    <table className="w-full text-left border-collapse text-xs">
                      <thead>
                        <tr className="border-b border-slate-800 bg-slate-900/90 text-slate-400">
                          <th className="py-2.5 px-3 font-medium">名稱</th>
                          <th className="py-2.5 px-3 font-medium w-28">類型</th>
                          <th className="py-2.5 px-3 font-medium w-28 text-right">
                            大小
                          </th>
                          <th className="py-2.5 px-3 font-medium w-40">
                            修改時間
                          </th>
                          <th className="py-2.5 px-3 font-medium w-44 text-right">
                            快速操作
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
                              <td className="py-2 px-3 flex items-center gap-2.5">
                                {item.is_dir ? (
                                  <Folder className="w-4 h-4 text-amber-400 shrink-0" />
                                ) : item.is_image ? (
                                  <ImageIcon className="w-4 h-4 text-pink-400 shrink-0" />
                                ) : item.category === 'code' ? (
                                  <FileCode className="w-4 h-4 text-cyan-400 shrink-0" />
                                ) : (
                                  <FileText className="w-4 h-4 text-slate-400 shrink-0" />
                                )}
                                <span className="truncate max-w-md font-medium">
                                  {item.name}
                                </span>
                                {item.is_image && (
                                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-pink-500/20 text-pink-300 border border-pink-500/30">
                                    可預覽圖片
                                  </span>
                                )}
                              </td>
                              <td className="py-2 px-3 text-slate-400">
                                {item.is_dir
                                  ? '資料夾'
                                  : item.extension.toUpperCase() || '檔案'}
                              </td>
                              <td className="py-2 px-3 text-right font-mono text-slate-300">
                                {item.is_dir
                                  ? '-'
                                  : formatBytes(item.size_bytes)}
                              </td>
                              <td className="py-2 px-3 text-slate-400">
                                {formatTimestamp(item.modified_unix)}
                              </td>
                              <td
                                className="py-2 px-3 text-right space-x-1"
                                onClick={(e) => e.stopPropagation()}
                              >
                                {item.is_dir && (
                                  <button
                                    onClick={() => loadDirectory(item.path)}
                                    className="px-2 py-1 rounded bg-slate-800 hover:bg-slate-700 text-slate-200"
                                  >
                                    開啟
                                  </button>
                                )}
                                <button
                                  onClick={() => {
                                    setLockTargetPath(item.path);
                                    setActiveTab('lockhunter');
                                    runLockHunter(item.path);
                                  }}
                                  className="px-2 py-1 rounded bg-amber-500/15 hover:bg-amber-500/30 text-amber-300"
                                  title="使用 LockHunter 查詢誰鎖住了此項目"
                                >
                                  解鎖
                                </button>
                                <button
                                  onClick={() => handleDeleteEntry(item)}
                                  className="px-2 py-1 rounded bg-rose-500/15 hover:bg-rose-500/30 text-rose-300"
                                  title="刪除項目"
                                >
                                  刪除
                                </button>
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                </div>

                {/* 右側圖片顯示與詳情檢視面板 */}
                <div className="w-96 border-l border-slate-800 bg-slate-900/60 p-4 flex flex-col gap-4 overflow-y-auto shrink-0">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2 font-semibold text-sm text-white">
                      <Eye className="w-4 h-4 text-pink-400" />
                      <span>圖片顯示與檔案檢視器</span>
                    </div>
                    {imagePreview?.data_url && (
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() =>
                            setImageZoom((z) => Math.max(50, z - 25))
                          }
                          className="p-1 rounded bg-slate-800 hover:bg-slate-700"
                          title="縮小圖片"
                        >
                          <ZoomOut className="w-3.5 h-3.5" />
                        </button>
                        <span className="text-[11px] text-slate-300 px-1">
                          {imageZoom}%
                        </span>
                        <button
                          onClick={() =>
                            setImageZoom((z) => Math.min(200, z + 25))
                          }
                          className="p-1 rounded bg-slate-800 hover:bg-slate-700"
                          title="放大圖片"
                        >
                          <ZoomIn className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    )}
                  </div>

                  {imagePreview ? (
                    <div className="space-y-3">
                      {imagePreview.deferred_for_network ? (
                        <div className="p-4 rounded-xl bg-amber-950/50 border border-amber-700/60 text-xs text-amber-200 space-y-3">
                          <div className="font-semibold flex items-center gap-1.5">
                            <ShieldAlert className="w-4 h-4 text-amber-400" />
                            <span>網路磁碟機大圖保護已觸發</span>
                          </div>
                          <p>{imagePreview.notice}</p>
                          {selectedEntry && (
                            <button
                              onClick={() =>
                                handleSelectEntry(selectedEntry, true)
                              }
                              className="w-full py-2 rounded-lg bg-amber-500 text-slate-950 font-semibold hover:bg-amber-400"
                            >
                              強制跨網路載入原圖預覽
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
                          <span>解析度尺寸：</span>
                          <span className="text-slate-200 font-mono">
                            {imagePreview.width} × {imagePreview.height} px (
                            {imagePreview.aspect_ratio})
                          </span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>影像格式 / 色彩：</span>
                          <span className="text-slate-200">
                            {imagePreview.format_name} ({imagePreview.color_type}
                            )
                          </span>
                        </div>
                        <div className="flex justify-between text-slate-400">
                          <span>檔案大小：</span>
                          <span className="text-slate-200 font-mono">
                            {formatBytes(imagePreview.file_size_bytes)}
                          </span>
                        </div>
                      </div>
                    </div>
                  ) : selectedEntry ? (
                    <div className="p-4 rounded-xl bg-slate-950 border border-slate-800 text-xs space-y-2">
                      <div className="font-semibold text-white">
                        {selectedEntry.name}
                      </div>
                      <div className="text-slate-400 break-all">
                        路徑：{selectedEntry.path}
                      </div>
                      <div className="text-slate-400">
                        大小：
                        {selectedEntry.is_dir
                          ? '資料夾'
                          : formatBytes(selectedEntry.size_bytes)}
                      </div>
                      <p className="text-slate-500 pt-2">
                        點選任何 PNG / JPG / GIF / WebP / BMP / ICO / SVG 圖片檔案即可在此即時渲染影像預覽與色彩資訊。
                      </p>
                    </div>
                  ) : (
                    <div className="p-6 rounded-xl border border-dashed border-slate-800 text-center text-xs text-slate-400 space-y-2">
                      <ImageIcon className="w-8 h-8 text-slate-600 mx-auto" />
                      <p>
                        從左側列表點選任何圖片或檔案，即可透過 Rust{' '}
                        <code>image</code> 引擎即時解碼顯示圖片與中繼資料。
                      </p>
                    </div>
                  )}

                  {/* 網路磁碟機永久刪除確認開關 */}
                  {activeCapabilities?.is_network_drive && (
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
                        我了解網路磁碟機不支援本機資源回收筒，允許在網路磁碟上執行永久刪除。
                      </span>
                    </label>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* ==================== 2. TreeSize 磁碟空間分析與 Treemap ==================== */}
          {activeTab === 'treesize' && (
            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              <div className="flex flex-wrap items-center justify-between gap-3 bg-slate-900/60 p-4 rounded-2xl border border-slate-800">
                <div className="flex-1 flex gap-2 min-w-[280px]">
                  <input
                    type="text"
                    value={treeSizePath}
                    onChange={(e) => setTreeSizePath(e.target.value)}
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100"
                    placeholder="輸入要分析空間的目錄路徑..."
                  />
                  <button
                    onClick={() => runTreeSize()}
                    className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <PieChart className="w-4 h-4" />
                    立即掃描分析
                  </button>
                </div>

                {treeSizeReport && (
                  <div className="flex items-center gap-4 text-xs">
                    <div>
                      <span className="text-slate-400">總容量：</span>
                      <span className="font-bold text-cyan-300">
                        {formatBytes(treeSizeReport.total_size_bytes)}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-400">檔案數：</span>
                      <span className="font-semibold text-white">
                        {treeSizeReport.total_files.toLocaleString()}
                      </span>
                    </div>
                    <div>
                      <span className="text-slate-400">耗時：</span>
                      <span className="font-mono text-emerald-300">
                        {treeSizeReport.scan_duration_ms} ms
                      </span>
                    </div>
                  </div>
                )}
              </div>

              {treeSizeReport && (
                <>
                  {/* 互動式 Treemap 矩形樹狀圖 */}
                  <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-semibold text-white flex items-center gap-2">
                        <Layers className="w-4 h-4 text-cyan-400" />
                        互動式 Treemap 空間佔比矩形圖（點擊子資料夾可下鑽分析）
                      </h3>
                      {treeSizeReport.capabilities.is_network_drive && (
                        <span className="text-xs px-2.5 py-0.5 rounded-full bg-amber-500/20 text-amber-300 border border-amber-500/40">
                          🌐 網路磁碟受控掃描 (最大深度{' '}
                          {treeSizeReport.capabilities.max_scan_depth} 層)
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
                                setTreeSizePath(node.path);
                                runTreeSize(node.path);
                              }
                            }}
                            style={{ flexGrow: Math.round(flexBasis), flexBasis: `${flexBasis}%` }}
                            className={`p-3 rounded-xl border text-left transition-transform hover:scale-[1.01] flex flex-col justify-between min-w-[140px] min-h-[84px] ${colorClass}`}
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
                              {formatBytes(node.size_bytes)} ({node.file_count}{' '}
                              檔)
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
                        依檔案類型空間分布
                      </h3>
                      <div className="space-y-2.5 text-xs">
                        {treeSizeReport.category_breakdown.map((cat) => (
                          <div key={cat.category} className="space-y-1">
                            <div className="flex justify-between">
                              <span className="text-slate-200 font-medium">
                                {cat.label} ({cat.file_count} 檔)
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
                        Top 20 巨型檔案排行榜
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
                            <span className="font-mono text-cyan-300 shrink-0">
                              {formatBytes(f.size_bytes)}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </>
              )}
            </div>
          )}

          {/* ==================== 3. 工作管理員 & Port Hunter ==================== */}
          {activeTab === 'taskmgr' && (
            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              {/* 硬體即時儀表板 */}
              {sysSnapshot && (
                <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>CPU 總負載</span>
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
                      <span>實體記憶體 (RAM)</span>
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
                      <span>活躍處理程序</span>
                      <Activity className="w-4 h-4 text-emerald-400" />
                    </div>
                    <div className="text-2xl font-bold text-white">
                      {sysSnapshot.total_processes}
                    </div>
                    <div className="text-[11px] text-slate-400 mt-1">
                      監聽連接埠：{ports.length} 個
                    </div>
                  </div>

                  <div className="p-4 rounded-2xl bg-slate-900/60 border border-slate-800">
                    <div className="flex items-center justify-between text-xs text-slate-400 mb-1">
                      <span>系統連續運作時間</span>
                      <RefreshCw className="w-4 h-4 text-amber-400" />
                    </div>
                    <div className="text-lg font-bold text-white">
                      {formatUptime(sysSnapshot.uptime_secs)}
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
                    處理程序清單 (Processes)
                  </button>
                  <button
                    onClick={() => setTaskSubTab('ports')}
                    className={`px-4 py-2 rounded-xl text-xs font-semibold ${
                      taskSubTab === 'ports'
                        ? 'bg-cyan-500 text-slate-950'
                        : 'bg-slate-900 text-slate-300 border border-slate-800'
                    }`}
                  >
                    🔌 Port 連接埠佔用獵人 ({ports.length})
                  </button>
                </div>

                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={taskSearch}
                    onChange={(e) => setTaskSearch(e.target.value)}
                    placeholder="搜尋 Process 名稱、PID 或 Port (如 8080)..."
                    className="bg-slate-900 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100 w-72"
                  />
                  <button
                    onClick={refreshTaskManager}
                    className="px-3.5 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-xs text-slate-200 flex items-center gap-1.5"
                  >
                    <RefreshCw className="w-3.5 h-3.5" />
                    重新整理
                  </button>
                </div>
              </div>

              {taskSubTab === 'processes' ? (
                <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                        <th className="py-2.5 px-3">PID</th>
                        <th className="py-2.5 px-3">處理程序名稱</th>
                        <th className="py-2.5 px-3 text-right">CPU %</th>
                        <th className="py-2.5 px-3 text-right">記憶體</th>
                        <th className="py-2.5 px-3">執行檔路徑</th>
                        <th className="py-2.5 px-3 text-right">操作</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-800/60">
                      {(sysSnapshot?.processes ?? [])
                        .filter(
                          (p) =>
                            !taskSearch.trim() ||
                            p.name
                              .toLowerCase()
                              .includes(taskSearch.toLowerCase()) ||
                            String(p.pid).includes(taskSearch.trim())
                        )
                        .map((proc) => (
                          <tr
                            key={proc.pid}
                            className="hover:bg-slate-800/50 text-slate-200"
                          >
                            <td className="py-2 px-3 font-mono text-cyan-300">
                              {proc.pid}
                            </td>
                            <td className="py-2 px-3 font-medium">
                              {proc.name}
                            </td>
                            <td className="py-2 px-3 text-right font-mono">
                              {proc.cpu_usage.toFixed(1)}%
                            </td>
                            <td className="py-2 px-3 text-right font-mono text-amber-300">
                              {formatBytes(proc.memory_bytes)}
                            </td>
                            <td className="py-2 px-3 text-slate-400 truncate max-w-xs">
                              {proc.exe_path || '-'}
                            </td>
                            <td className="py-2 px-3 text-right">
                              <button
                                onClick={() => handleKillProcess(proc.pid)}
                                className="px-2.5 py-1 rounded bg-rose-500/20 hover:bg-rose-500/40 text-rose-300"
                              >
                                終止行程
                              </button>
                            </td>
                          </tr>
                        ))}
                    </tbody>
                  </table>
                </div>
              ) : (
                <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                  <table className="w-full text-left text-xs">
                    <thead>
                      <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                        <th className="py-2.5 px-3">協定</th>
                        <th className="py-2.5 px-3">本機 Port</th>
                        <th className="py-2.5 px-3">本機位址</th>
                        <th className="py-2.5 px-3">狀態</th>
                        <th className="py-2.5 px-3">PID & 佔用程式</th>
                        <th className="py-2.5 px-3 text-right">釋放連接埠</th>
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
                                  釋放 Port (Kill)
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

          {/* ==================== 4. LockHunter 檔案佔用解鎖器 ==================== */}
          {activeTab === 'lockhunter' && (
            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-4 space-y-3">
                <h2 className="text-sm font-semibold text-white flex items-center gap-2">
                  <Unlock className="w-4 h-4 text-amber-400" />
                  LockHunter 檔案與資料夾佔用偵測 (Windows Restart Manager + 行程句柄分析)
                </h2>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={lockTargetPath}
                    onChange={(e) => setLockTargetPath(e.target.value)}
                    placeholder="輸入無法刪除或移動的檔案/資料夾路徑..."
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100"
                  />
                  <button
                    onClick={() => runLockHunter()}
                    className="px-4 py-2 rounded-xl bg-amber-500 hover:bg-amber-400 text-slate-950 font-semibold text-xs"
                  >
                    掃描佔用行程
                  </button>
                </div>
              </div>

              {lockReport && (
                <div className="space-y-4">
                  <div className="p-4 rounded-2xl border border-slate-800 bg-slate-900/40 flex flex-wrap items-center justify-between gap-4">
                    <div className="space-y-1">
                      <div className="flex items-center gap-2">
                        {lockReport.is_locked ? (
                          <Lock className="w-4 h-4 text-rose-400" />
                        ) : (
                          <Unlock className="w-4 h-4 text-emerald-400" />
                        )}
                        <span className="font-semibold text-sm text-white">
                          {lockReport.is_locked
                            ? `發現 ${lockReport.locking_processes.length} 個處理程序正在佔用此路徑`
                            : '目前未偵測到任何本機行程鎖定此路徑，可安全操作'}
                        </span>
                        <span
                          className={`text-xs px-2 py-0.5 rounded-full ${
                            lockReport.scope === 'local_process_only'
                              ? 'bg-amber-500/20 text-amber-300 border border-amber-500/40'
                              : 'bg-emerald-500/20 text-emerald-300'
                          }`}
                        >
                          {lockReport.scope === 'local_process_only'
                            ? '🌐 網路磁碟：僅限本機行程範圍'
                            : '💻 本機完整支援'}
                        </span>
                      </div>
                      <div className="text-xs text-slate-400">
                        已檢查檔案數：{lockReport.scanned_files_count} 個
                      </div>
                    </div>

                    <button
                      disabled={!lockReport.capabilities.supports_boot_delete}
                      onClick={async () => {
                        try {
                          const msg = await keeperApi.scheduleBootDelete(
                            lockReport.target_path
                          );
                          notifyStatus(msg);
                        } catch (err) {
                          notifyError(err);
                        }
                      }}
                      className="px-3.5 py-2 rounded-xl bg-rose-500/20 border border-rose-500/40 text-rose-200 hover:bg-rose-500/30 disabled:opacity-40 disabled:cursor-not-allowed text-xs"
                    >
                      {lockReport.capabilities.supports_boot_delete
                        ? '排程於下次重開機時強制刪除'
                        : '🌐 網路磁碟不支援重開機刪除'}
                    </button>
                  </div>

                  <div className="space-y-1.5 text-xs text-slate-400">
                    {lockReport.advisory_notes.map((n, idx) => (
                      <div key={idx}>{n}</div>
                    ))}
                  </div>

                  {lockReport.locking_processes.length > 0 && (
                    <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                      <table className="w-full text-left text-xs">
                        <thead>
                          <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                            <th className="py-2.5 px-3">PID</th>
                            <th className="py-2.5 px-3">行程名稱</th>
                            <th className="py-2.5 px-3">類型</th>
                            <th className="py-2.5 px-3">鎖定原因</th>
                            <th className="py-2.5 px-3 text-right">解鎖動作</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-slate-800/60">
                          {lockReport.locking_processes.map((lp) => (
                            <tr key={lp.pid} className="hover:bg-slate-800/50">
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
                                {lp.lock_reason}
                              </td>
                              <td className="py-2.5 px-3 text-right">
                                <button
                                  onClick={async () => {
                                    await handleKillProcess(lp.pid);
                                    await runLockHunter();
                                  }}
                                  className="px-3 py-1 rounded-lg bg-rose-500 text-white font-semibold hover:bg-rose-400"
                                >
                                  強制終止並解鎖
                                </button>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}

          {/* ==================== 5. FreeFileSync 智慧檔案同步 ==================== */}
          {activeTab === 'sync' && (
            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-4 space-y-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h2 className="text-sm font-semibold text-white flex items-center gap-2">
                    <FolderSync className="w-4 h-4 text-cyan-400" />
                    FreeFileSync 雙目錄差異比對與智慧同步（內建網路磁碟 2 秒 SMB 時間容差）
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
                        比對模式：修改時間 + 檔案大小 (極速)
                      </option>
                      <option value="blake3_hash">
                        比對模式：BLAKE3 內容雜湊 (精確驗證)
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
                        同步策略：鏡像 (Mirror 左 → 右)
                      </option>
                      <option value="update">
                        同步策略：更新 (Update 左 → 右，不刪除)
                      </option>
                      <option value="two_way">
                        同步策略：雙向互補 (Two-Way)
                      </option>
                    </select>
                  </div>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">
                      左側來源目錄 (Source / Left)
                    </label>
                    <input
                      type="text"
                      value={syncLeftPath}
                      onChange={(e) => setSyncLeftPath(e.target.value)}
                      className="w-full bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100"
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-400 mb-1">
                      右側目標目錄 (Target / Right — 支援本機或 \\NAS\Backup)
                    </label>
                    <input
                      type="text"
                      value={syncRightPath}
                      onChange={(e) => setSyncRightPath(e.target.value)}
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
                    1. 比對左右目錄差異
                  </button>
                  <button
                    onClick={() => runSyncExecute(true)}
                    disabled={!syncReport}
                    className="px-4 py-2 rounded-xl bg-indigo-500/20 border border-indigo-500/40 text-indigo-200 hover:bg-indigo-500/30 disabled:opacity-40 text-xs font-medium"
                  >
                    2. 預演同步 (Dry-Run)
                  </button>
                  <button
                    onClick={() => runSyncExecute(false)}
                    disabled={!syncReport}
                    className="px-4 py-2 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-slate-950 font-semibold disabled:opacity-40 text-xs"
                  >
                    3. 立即執行同步
                  </button>
                </div>
              </div>

              {syncExecResult && (
                <div className="p-4 rounded-2xl bg-emerald-950/50 border border-emerald-700/60 text-xs text-emerald-200 flex flex-wrap items-center justify-between gap-4">
                  <div>
                    <strong>
                      {syncExecResult.dry_run ? '預演結果：' : '同步完成：'}
                    </strong>{' '}
                    左→右複製 {syncExecResult.copied_left_to_right} 檔 · 右→左複製{' '}
                    {syncExecResult.copied_right_to_left} 檔 · 右側清理{' '}
                    {syncExecResult.deleted_right} 檔（傳輸總量{' '}
                    {formatBytes(syncExecResult.bytes_copied)}）
                  </div>
                </div>
              )}

              {syncReport && (
                <div className="space-y-4">
                  {syncReport.network_advisory && (
                    <div className="p-3 rounded-xl bg-amber-950/60 border border-amber-700/60 text-xs text-amber-200">
                      {syncReport.network_advisory}
                    </div>
                  )}

                  <div className="grid grid-cols-2 md:grid-cols-6 gap-3 text-xs">
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">僅左側存在</div>
                      <div className="text-lg font-bold text-cyan-300">
                        {syncReport.only_left_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">僅右側存在</div>
                      <div className="text-lg font-bold text-amber-300">
                        {syncReport.only_right_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">左側較新</div>
                      <div className="text-lg font-bold text-emerald-300">
                        {syncReport.left_newer_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">右側較新</div>
                      <div className="text-lg font-bold text-indigo-300">
                        {syncReport.right_newer_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">內容不同</div>
                      <div className="text-lg font-bold text-rose-300">
                        {syncReport.different_content_count}
                      </div>
                    </div>
                    <div className="p-3 rounded-xl bg-slate-900 border border-slate-800">
                      <div className="text-slate-400">完全相同</div>
                      <div className="text-lg font-bold text-slate-300">
                        {syncReport.identical_count}
                      </div>
                    </div>
                  </div>

                  <div className="border border-slate-800 rounded-2xl overflow-hidden bg-slate-900/40">
                    <table className="w-full text-left text-xs">
                      <thead>
                        <tr className="bg-slate-900 border-b border-slate-800 text-slate-400">
                          <th className="py-2.5 px-3">相對路徑</th>
                          <th className="py-2.5 px-3 text-right">左側大小</th>
                          <th className="py-2.5 px-3 text-center">差異狀態</th>
                          <th className="py-2.5 px-3 text-right">右側大小</th>
                          <th className="py-2.5 px-3 text-right">預定同步動作</th>
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

          {/* ==================== 6. 電腦本體維運中心 (System Ops) ==================== */}
          {activeTab === 'ops' && (
            <div className="flex-1 overflow-y-auto p-5 space-y-5">
              {/* 開發垃圾與重複檔案/圖片掃描列 */}
              <div className="bg-slate-900/60 border border-slate-800 rounded-2xl p-4 space-y-3">
                <h2 className="text-sm font-semibold text-white flex items-center gap-2">
                  <Sparkles className="w-4 h-4 text-cyan-400" />
                  開發者建置快取清理 (`target` / `node_modules` / `.venv`) & 重複檔案/重複圖片查殺
                </h2>
                <div className="flex flex-wrap gap-2">
                  <input
                    type="text"
                    value={opsTargetPath}
                    onChange={(e) => setOpsTargetPath(e.target.value)}
                    placeholder="輸入要掃描的工作區目錄路徑..."
                    className="flex-1 bg-slate-950 border border-slate-800 rounded-xl px-3.5 py-2 text-xs text-slate-100 min-w-[260px]"
                  />
                  <button
                    onClick={runJunkScan}
                    className="px-4 py-2 rounded-xl bg-cyan-500 hover:bg-cyan-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <Trash2 className="w-4 h-4" />
                    掃描開發建置垃圾
                  </button>
                  <button
                    onClick={runDuplicateScan}
                    className="px-4 py-2 rounded-xl bg-pink-500 hover:bg-pink-400 text-slate-950 font-semibold text-xs flex items-center gap-1.5"
                  >
                    <ImageIcon className="w-4 h-4" />
                    掃描重複檔案与圖片 (BLAKE3)
                  </button>
                </div>
              </div>

              {/* 開發建置垃圾結果 */}
              {junkReport && (
                <div className="bg-slate-900/50 border border-slate-800 rounded-2xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <h3 className="text-sm font-semibold text-white">
                      發現可回收的開發者快取目錄（共計{' '}
                      <span className="text-cyan-300">
                        {formatBytes(junkReport.total_reclaimable_bytes)}
                      </span>
                      ）
                    </h3>
                  </div>
                  {junkReport.items.length === 0 ? (
                    <p className="text-xs text-slate-400">
                      此目錄下未發現大型開發快取目錄。
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
                              {item.kind} — {item.description}
                            </div>
                            <div className="text-slate-400 font-mono">
                              {item.path} ({item.file_count} 個檔案)
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
                    BLAKE3 重複檔案與重複圖片報表（共 {dupReport.total_groups}{' '}
                    組，可省下{' '}
                    <span className="text-pink-300">
                      {formatBytes(dupReport.total_wasted_bytes)}
                    </span>
                    ）
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
                            {g.is_image && '🖼️ (重複圖片)'}
                          </span>
                          <span className="text-amber-300 font-semibold">
                            單檔 {formatBytes(g.file_size_bytes)} · 浪費{' '}
                            {formatBytes(g.wasted_bytes)}
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
                      💽 磁碟健康度 (S.M.A.R.T.) 與系統暫存區狀態
                    </h3>
                    <div className="p-3 rounded-xl bg-slate-950 border border-slate-800 text-xs flex justify-between">
                      <span className="text-slate-400">
                        Windows Temp 暫存目錄 ({maintOverview.temp_dir_path})：
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
                              {d.mount_point} ({d.name})
                            </span>
                            <span className="text-xs">{d.health_badge}</span>
                          </div>
                          <div className="text-slate-400">{d.note}</div>
                        </div>
                      ))}
                    </div>

                    <h4 className="text-xs font-semibold text-slate-300 pt-2">
                      🚀 偵測到的使用者開機啟動項 ({maintOverview.startup_entries.length})
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
                        🩺 系統 PATH 環境變數體檢
                      </h3>
                      <span className="text-xs text-amber-300">
                        失效路徑：{maintOverview.broken_path_count} 筆 · 網路路徑：
                        {maintOverview.network_path_count} 筆
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
                            {pd.note}
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
    </div>
  );
}
export default App;

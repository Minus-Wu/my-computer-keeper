import type {
  CompareMode,
  DirectoryListing,
  DriveInfo,
  DuplicateScanReport,
  GeneralItemPreview,
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
} from '../types/keeper';

interface ApiEnvelope<T> {
  ok: boolean;
  data: T | null;
  error: string | null;
}

const API_BASE = import.meta.env.VITE_KEEPER_API_BASE ?? '';

export async function invokeKeeper<T>(
  cmd: string,
  payload: Record<string, unknown> = {}
): Promise<T> {
  const response = await fetch(`${API_BASE}/api/${cmd}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    throw new Error(`Rust 核心通訊錯誤 (HTTP ${response.status})`);
  }

  const envelope = (await response.json()) as ApiEnvelope<T>;
  if (!envelope.ok || envelope.data === null) {
    throw new Error(envelope.error ?? `執行指令 ${cmd} 失敗`);
  }

  return envelope.data;
}

export const keeperApi = {
  getDrives: () => invokeKeeper<DriveInfo[]>('get_drives'),
  inspectCapabilities: (path: string) =>
    invokeKeeper<VolumeCapabilities>('inspect_capabilities', { path }),
  listDirectory: (path?: string) =>
    invokeKeeper<DirectoryListing>('list_directory', { path: path ?? '' }),
  previewImage: (
    path: string,
    maxDimension = 960,
    forceNetworkLoad = false
  ) =>
    invokeKeeper<ImagePreviewResult>('preview_image', {
      path,
      max_dimension: maxDimension,
      force_network_load: forceNetworkLoad,
    }),
  previewEntry: (path: string, forceNetworkLoad = false) =>
    invokeKeeper<GeneralItemPreview>('preview_entry', {
      path,
      force_network_load: forceNetworkLoad,
    }),

  deleteEntry: (
    path: string,
    useRecycleBin: boolean,
    confirmPermanentOnNetwork: boolean
  ) =>
    invokeKeeper<string>('delete_entry', {
      path,
      use_recycle_bin: useRecycleBin,
      confirm_permanent_on_network: confirmPermanentOnNetwork,
    }),
  analyzeTreeSize: (path: string, maxDepth?: number) =>
    invokeKeeper<TreeSizeReport>('analyze_treesize', {
      path,
      max_depth: maxDepth,
    }),
  getSystemSnapshot: (maxProcesses = 80) =>
    invokeKeeper<SystemSnapshot>('get_system_snapshot', {
      max_processes: maxProcesses,
    }),
  killProcess: (pid: number) => invokeKeeper<string>('kill_process', { pid }),
  listPorts: () => invokeKeeper<PortBindingEntry[]>('list_ports'),
  inspectLocks: (path: string) =>
    invokeKeeper<LockHunterReport>('inspect_locks', { path }),
  scheduleBootDelete: (path: string) =>
    invokeKeeper<string>('schedule_boot_delete', { path }),
  compareSync: (
    leftPath: string,
    rightPath: string,
    compareMode: CompareMode,
    strategy: SyncStrategy
  ) =>
    invokeKeeper<SyncComparisonReport>('compare_sync', {
      left_path: leftPath,
      right_path: rightPath,
      compare_mode: compareMode,
      strategy,
    }),
  executeSync: (
    leftPath: string,
    rightPath: string,
    compareMode: CompareMode,
    strategy: SyncStrategy,
    dryRun: boolean
  ) =>
    invokeKeeper<SyncExecutionSummary>('execute_sync', {
      left_path: leftPath,
      right_path: rightPath,
      compare_mode: compareMode,
      strategy,
      dry_run: dryRun,
    }),
  findDuplicates: (path: string) =>
    invokeKeeper<DuplicateScanReport>('find_duplicates', { path }),
  scanJunk: (path: string) => invokeKeeper<JunkScanReport>('scan_junk', { path }),
  inspectMaintenance: () =>
    invokeKeeper<SystemMaintenanceOverview>('inspect_maintenance'),
};

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1
  );
  const val = bytes / Math.pow(1024, i);
  return `${val >= 100 || i === 0 ? val.toFixed(0) : val.toFixed(1)} ${units[i]}`;
}

export function formatTimestamp(unixSecs: number | null): string {
  if (!unixSecs) return '-';
  const d = new Date(unixSecs * 1000);
  return d.toLocaleString('zh-TW', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return `${days} 天 ${hours} 小時 ${mins} 分`;
  return `${hours} 小時 ${mins} 分`;
}

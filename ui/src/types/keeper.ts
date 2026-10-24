export type DriveKind =
  | 'fixed'
  | 'removable'
  | 'network'
  | 'cd_rom'
  | 'ram_disk'
  | 'unknown';

export type LockHunterScope = 'full_local' | 'local_process_only';

export interface VolumeCapabilities {
  drive_kind: DriveKind;
  is_network_drive: boolean;
  remote_unc_path: string | null;
  supports_mft_fast_scan: boolean;
  supports_recycle_bin: boolean;
  supports_smart_health: boolean;
  supports_boot_delete: boolean;
  lockhunter_scope: LockHunterScope;
  max_scan_threads: number;
  max_scan_depth: number;
  max_scan_entries: number;
  max_auto_preview_bytes: number;
  sync_timestamp_tolerance_secs: number;
  limitations_notice: string[];
}

export interface DriveInfo {
  mount_point: string;
  name: string;
  file_system: string;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  usage_percent: number;
  capabilities: VolumeCapabilities;
}

export type FileCategory =
  | 'directory'
  | 'image'
  | 'video'
  | 'audio'
  | 'code'
  | 'archive'
  | 'document'
  | 'executable'
  | 'other';

export interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_symlink: boolean;
  size_bytes: number;
  modified_unix: number;
  extension: string;
  category: FileCategory;
  is_image: boolean;
  readonly: boolean;
}

export interface DirectoryListing {
  current_path: string;
  parent_path: string | null;
  capabilities: VolumeCapabilities;
  total_dirs: number;
  total_files: number;
  total_images: number;
  total_size_bytes: number;
  entries: FileEntry[];
}

export interface ImagePreviewResult {
  path: string;
  name: string;
  file_size_bytes: number;
  width: number;
  height: number;
  format_name: string;
  color_type: string;
  aspect_ratio: string;
  data_url: string | null;
  deferred_for_network: boolean;
  notice: string | null;
}

export interface KnownFolderInfo {
  is_system_or_known: boolean;
  category_label: string;
  badge_color: string;
  summary: string;
  description: string;
  safety_warning: string | null;
}

export interface DirectoryPreviewResult {
  name: string;
  path: string;
  is_system_or_known: boolean;
  category_label: string;
  badge_color: string;
  summary: string;
  description: string;
  safety_warning: string | null;
  direct_subdirs_count: number;
  direct_files_count: number;
  direct_size_bytes: number;
  has_permission_error: boolean;
}

export interface TextPreviewResult {
  content: string;
  total_lines: number;
  preview_lines: number;
  is_truncated: boolean;
  syntax: string;
  encoding: string;
}

export interface ZipEntryItem {
  name: string;
  is_dir: boolean;
  uncompressed_size: number;
  compressed_size: number;
  compression_ratio: number;
}

export interface ZipPreviewResult {
  total_entries: number;
  total_files: number;
  total_dirs: number;
  total_uncompressed_bytes: number;
  total_compressed_bytes: number;
  overall_ratio: number;
  entries: ZipEntryItem[];
  comment: string | null;
}

export interface PdfPreviewResult {
  data_url: string | null;
  file_size_bytes: number;
  version: string;
  notice: string | null;
  deferred_for_network: boolean;
}

export interface MediaPreviewResult {
  media_type: 'audio' | 'video';
  format: string;
  data_url: string | null;
  file_size_bytes: number;
  notice: string | null;
}

export interface HexRow {
  offset: string;
  hex: string;
  ascii: string;
}

export interface HexPreviewResult {
  file_size_bytes: number;
  rows: HexRow[];
  magic_description: string | null;
}

export type PreviewKind =
  | 'directory'
  | 'image'
  | 'text'
  | 'pdf'
  | 'zip'
  | 'media'
  | 'hex';

export interface GeneralItemPreview {
  path: string;
  name: string;
  is_dir: boolean;
  extension: string;
  file_size_bytes: number;
  modified_unix: number;
  preview_kind: PreviewKind;
  dir_preview: DirectoryPreviewResult | null;
  image_preview: ImagePreviewResult | null;
  text_preview: TextPreviewResult | null;
  pdf_preview: PdfPreviewResult | null;
  zip_preview: ZipPreviewResult | null;
  media_preview: MediaPreviewResult | null;
  hex_preview: HexPreviewResult | null;
  notice: string | null;
}


export interface TreeSizeNode {
  name: string;
  path: string;
  is_dir: boolean;
  size_bytes: number;
  file_count: number;
  percentage: number;
  category: FileCategory;
}

export interface CategoryStat {
  category: FileCategory;
  label: string;
  size_bytes: number;
  file_count: number;
  percentage: number;
}

export interface LargestFileItem {
  name: string;
  path: string;
  size_bytes: number;
  category: FileCategory;
  is_image: boolean;
}

export interface TreeSizeReport {
  root_path: string;
  parent_path: string | null;
  capabilities: VolumeCapabilities;
  total_size_bytes: number;
  total_files: number;
  total_dirs: number;
  scan_duration_ms: number;
  truncated_by_safety_limit: boolean;
  drive_total_bytes?: number | null;
  drive_free_bytes?: number | null;
  drive_used_bytes?: number | null;
  is_drive_root?: boolean;
  children: TreeSizeNode[];
  category_breakdown: CategoryStat[];
  largest_files: LargestFileItem[];
}

export type TreeSizeScanStatus = 'scanning' | 'completed' | 'cancelled' | 'failed';

export interface TreeSizeProgress {
  current_path: string;
  processed_files: number;
  processed_dirs: number;
  processed_bytes: number;
  elapsed_ms: number;
  estimated_percent: number | null;
}

export interface TreeSizeScanState {
  scan_id: string;
  target_path: string;
  status: TreeSizeScanStatus;
  progress: TreeSizeProgress;
  report: TreeSizeReport | null;
  error: string | null;
}

export interface ProcessEntry {
  pid: number;
  parent_pid: number | null;
  name: string;
  cpu_usage: number;
  memory_bytes: number;
  disk_read_bytes: number;
  disk_written_bytes: number;
  status: string;
  exe_path: string;
}

export interface SystemSnapshot {
  host_name: string;
  os_version: string;
  uptime_secs: number;
  cpu_brand: string;
  global_cpu_usage: number;
  per_core_usage: number[];
  total_memory_bytes: number;
  used_memory_bytes: number;
  total_swap_bytes: number;
  used_swap_bytes: number;
  total_processes: number;
  processes: ProcessEntry[];
}

export interface PortBindingEntry {
  protocol: string;
  local_address: string;
  local_port: number;
  remote_address: string;
  state: string;
  pid: number;
  process_name: string;
  exe_path: string;
}

export interface LockingProcessInfo {
  pid: number;
  process_name: string;
  app_type: string;
  exe_path: string;
  lock_reason: string;
  restartable: boolean;
}

export interface LockHunterReport {
  target_path: string;
  capabilities: VolumeCapabilities;
  scope: LockHunterScope;
  is_locked: boolean;
  scanned_files_count: number;
  locking_processes: LockingProcessInfo[];
  advisory_notes: string[];
}

export type CompareMode = 'time_and_size' | 'blake3_hash';
export type SyncStrategy = 'mirror' | 'update' | 'two_way';
export type DiffStatus =
  | 'only_left'
  | 'only_right'
  | 'left_newer'
  | 'right_newer'
  | 'different_content'
  | 'identical';

export type SyncAction =
  | 'copy_left_to_right'
  | 'copy_right_to_left'
  | 'delete_right'
  | 'skip_identical'
  | 'conflict_manual';

export interface SyncDiffItem {
  relative_path: string;
  left_size: number | null;
  right_size: number | null;
  left_modified_unix: number | null;
  right_modified_unix: number | null;
  status: DiffStatus;
  proposed_action: SyncAction;
}

export interface SyncComparisonReport {
  left_root: string;
  right_root: string;
  left_capabilities: VolumeCapabilities;
  right_capabilities: VolumeCapabilities;
  compare_mode: CompareMode;
  strategy: SyncStrategy;
  timestamp_tolerance_secs: number;
  only_left_count: number;
  only_right_count: number;
  left_newer_count: number;
  right_newer_count: number;
  different_content_count: number;
  identical_count: number;
  bytes_to_transfer: number;
  network_advisory: string | null;
  items: SyncDiffItem[];
}

export interface SyncExecutionSummary {
  dry_run: boolean;
  copied_left_to_right: number;
  copied_right_to_left: number;
  deleted_right: number;
  bytes_copied: number;
  errors: string[];
}

export interface DuplicateGroup {
  blake3_hash: string;
  file_size_bytes: number;
  wasted_bytes: number;
  is_image: boolean;
  files: string[];
}

export interface DuplicateScanReport {
  root_path: string;
  capabilities: VolumeCapabilities;
  total_groups: number;
  total_wasted_bytes: number;
  groups: DuplicateGroup[];
}

export interface JunkArtifactItem {
  kind: string;
  path: string;
  size_bytes: number;
  file_count: number;
  description: string;
}

export interface JunkScanReport {
  scanned_root: string;
  total_reclaimable_bytes: number;
  items: JunkArtifactItem[];
}

export type PathHealthStatus =
  | 'valid_local'
  | 'missing_directory'
  | 'network_path_warning';

export interface EnvPathDiagnosis {
  index: number;
  path: string;
  status: PathHealthStatus;
  note: string;
}

export interface StartupEntry {
  name: string;
  command_or_path: string;
  source: string;
}

export interface DiskSmartSummary {
  mount_point: string;
  name: string;
  is_network_drive: boolean;
  smart_supported: boolean;
  health_badge: string;
  usage_percent: number;
  free_bytes: number;
  note: string;
}

export interface SystemMaintenanceOverview {
  path_diagnostics: EnvPathDiagnosis[];
  broken_path_count: number;
  network_path_count: number;
  startup_entries: StartupEntry[];
  disk_health_summaries: DiskSmartSummary[];
  temp_dir_path: string;
  temp_dir_size_bytes: number;
}

export interface SafeEjectReport {
  success: boolean;
  drive_letter: string;
  disk_number: number | null;
  pnp_ejected: boolean;
  flush_succeeded: boolean;
  dismount_succeeded: boolean;
  has_system_partition: boolean;
  safe_to_unplug: boolean;
  veto_reason: string | null;
  message: string;
  advisory: string[];
}

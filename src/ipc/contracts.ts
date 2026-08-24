// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

export type OutputFormat = 'mp4' | 'mov' | 'mp3' | 'flac';

export type VideoQuality =
  | 'p144'
  | 'p240'
  | 'p360'
  | 'p480'
  | 'p720'
  | 'p1080'
  | 'p1440'
  | 'p2160'
  | 'best';

export type Mp3Quality = 'k128' | 'k192' | 'k256' | 'k320';

export type ThemeMode = 'light' | 'dark' | 'system';

export type Language = 'fr' | 'en';

export type DownloadStatus =
  | 'queued'
  | 'preparing'
  | 'probing'
  | 'downloading'
  | 'converting'
  | 'finalizing'
  | 'completed'
  | 'failed'
  | 'canceled';

export type ProgressPhase =
  | 'preparing'
  | 'probing'
  | 'downloading'
  | 'converting'
  | 'finalizing';

export type DownloadErrorCode =
  | 'YTDLP_NOT_FOUND'
  | 'YTDLP_START_FAILED'
  | 'YTDLP_UPDATE_REQUIRED'
  | 'NETWORK_UNAVAILABLE'
  | 'VIDEO_UNAVAILABLE'
  | 'AUTHENTICATION_REQUIRED'
  | 'RATE_LIMITED'
  | 'FORMAT_NOT_AVAILABLE'
  | 'FFMPEG_NOT_FOUND'
  | 'FFMPEG_FAILED'
  | 'OUTPUT_PERMISSION_DENIED'
  | 'OUTPUT_FILE_NOT_FOUND'
  | 'SOURCE_URL_INVALID'
  | 'HISTORY_SAVE_FAILED'
  | 'DISK_FULL'
  | 'DOWNLOAD_CANCELED'
  | 'DOWNLOAD_PROCESS_FAILED';

export interface DownloadErrorDetails {
  code: DownloadErrorCode;
  message: string;
  retryable: boolean;
  component?: string;
  exitCode?: number;
  stderrTail?: string;
}

export interface DownloadPresetDto {
  format: OutputFormat;
  videoQuality?: VideoQuality;
  mp3Quality?: Mp3Quality;
}

export interface AppSettingsDto {
  downloadDirectory: string;
  themeMode: ThemeMode;
  parallelDownloads: boolean;
  defaultPreset: DownloadPresetDto;
  maxConcurrent: number;
  language?: Language;
}

export interface FormatOption {
  formatId: string;
  height?: number | null;
  hasVideo: boolean;
  hasAudio: boolean;
  extension: string;
  filesizeApproxBytes?: number | null;
  note?: string | null;
}

export interface ProbeResult {
  url: string;
  title: string;
  durationSeconds?: number | null;
  thumbnailUrl?: string | null;
  uploader?: string | null;
  formats: FormatOption[];
  availableVideoQualities: VideoQuality[];
}

export interface AvailabilityStatus {
  isReady: boolean;
  version?: string;
  binaryPath?: string;
  statusMessage: string;
}

export interface HealthStatus {
  coreStatus: string;
  ytdlp: AvailabilityStatus;
  ffmpeg: AvailabilityStatus;
}

export interface DownloadJobDto {
  id: string;
  url: string;
  preset: DownloadPresetDto;
  title?: string | null;
  status: DownloadStatus;
  progressPercent?: number | null;
  downloadedBytes?: number | null;
  totalBytes?: number | null;
  speedBytesPerSecond?: number | null;
  destinationPath?: string | null;
  errorMessage?: string | null;
  errorDetails?: DownloadErrorDetails | null;
}

export interface DownloadHistoryEntryDto {
  id: string;
  downloadId: string;
  sourceUrl: string;
  title: string;
  preset: DownloadPresetDto;
  destinationPath: string;
  completedAt: number;
}

export interface DownloadProgressEvent {
  downloadId: string;
  phase: ProgressPhase;
  percent?: number | null;
  downloadedBytes?: number | null;
  totalBytes?: number | null;
  speedBytesPerSecond?: number | null;
}

export interface DownloadWarningEvent {
  downloadId: string;
  code: string;
  message: string;
}

export interface AppError {
  code: string;
  message: string;
  retryable?: boolean;
  details?: DownloadErrorDetails;
}

export interface StartDownloadRequestDto {
  url: string;
  preset?: DownloadPresetDto;
  outputDirectory?: string;
}

export interface AnalyzeUrlRequest {
  url: string;
}

export interface SetSettingsRequest {
  settings: AppSettingsDto;
}

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  body?: string;
  date?: string;
}

export type UpdateProgressCallback = (downloadedBytes: number, totalBytes: number | null) => void;

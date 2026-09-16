// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import type {
  AppError,
  AppSettingsDto,
  DownloadErrorDetails,
  DownloadHistoryEntryDto,
  DownloadJobDto,
  DownloadPresetDto,
  EngineUpdateResultDto,
  EngineUpdateStatusDto,
  HealthStatus,
  JsRuntimeStatusDto,
  PlaylistDetectionDto,
  ProbeResult,
  UpdateInfo,
  UpdateProgressCallback,
} from './contracts';

/**
 * Normalizes any unknown rejection into a structured AppError.
 *
 * The backend `IpcError` payload is flat: `{ code, message, retryable, details? }`
 * where `details` carries `component`, `exitCode` and `stderrTail`. All of these
 * are preserved so the UI can show localized messages *and* technical details.
 */
export function normalizeIpcError(err: unknown): AppError {
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    const candidate = err as Record<string, unknown>;
    const out: AppError = {
      code: String(candidate.code),
      message: String(candidate.message),
    };
    if (typeof candidate.retryable === 'boolean') {
      out.retryable = candidate.retryable;
    }
    if (typeof candidate.details === 'object' && candidate.details !== null) {
      out.details = candidate.details as DownloadErrorDetails;
    }
    // Tolerate flat variants (component/exitCode/stderrTail at top level).
    if (
      out.details === undefined &&
      (typeof candidate.component === 'string' ||
        typeof candidate.exitCode === 'number' ||
        typeof candidate.stderrTail === 'string')
    ) {
      out.details = {
        code: out.code as DownloadErrorDetails['code'],
        message: out.message,
        retryable: out.retryable ?? false,
        component: typeof candidate.component === 'string' ? candidate.component : undefined,
        exitCode: typeof candidate.exitCode === 'number' ? candidate.exitCode : undefined,
        stderrTail: typeof candidate.stderrTail === 'string' ? candidate.stderrTail : undefined,
      };
    }
    return out;
  }

  if (err instanceof Error) {
    return {
      code: 'UNKNOWN_ERROR',
      message: err.message,
    };
  }

  return {
    code: 'UNKNOWN_ERROR',
    message: typeof err === 'string' ? err : 'Une erreur inattendue est survenue',
  };
}

export interface IpcClient {
  healthCheck(): Promise<HealthStatus>;
  analyzeUrl(url: string): Promise<ProbeResult>;
  cancelAnalyze(): Promise<void>;
  /**
   * Asks the download engine whether an URL is a playlist.
   *
   * This is the native answer (yt-dlp reads the real listing), not a guess based
   * on the URL shape, and it downloads nothing.
   */
  detectPlaylist(url: string): Promise<PlaylistDetectionDto>;
  cancelPlaylistDetection(): Promise<void>;
  getSettings(): Promise<AppSettingsDto>;
  setSettings(settings: AppSettingsDto): Promise<AppSettingsDto>;
  startDownload(
    url: string,
    preset?: DownloadPresetDto,
    outputDirectory?: string,
  ): Promise<DownloadJobDto>;
  /**
   * Starts one job per selected playlist entry.
   *
   * Every URL is re-validated by the backend before any job is created; this call
   * only carries the user's selection.
   */
  startPlaylistDownload(
    url: string,
    preset: DownloadPresetDto | undefined,
    outputDirectory: string | undefined,
    selectedUrls: string[],
  ): Promise<DownloadJobDto[]>;
  listDownloads(): Promise<DownloadJobDto[]>;
  cancelDownload(downloadId: string): Promise<DownloadJobDto>;
  retryDownload(downloadId: string): Promise<DownloadJobDto>;
  dismissDownload(downloadId: string): Promise<void>;
  openDownloadSourceUrl(downloadId: string): Promise<void>;
  pickDirectory(defaultPath?: string): Promise<string | null>;
  revealDownloadedFile(downloadId: string): Promise<void>;
  openDownloadedFile(downloadId: string): Promise<void>;
  listDownloadHistory(): Promise<DownloadHistoryEntryDto[]>;
  removeDownloadHistoryEntry(historyId: string): Promise<void>;
  revealHistoryFile(historyId: string): Promise<void>;
  openHistoryFile(historyId: string): Promise<void>;
  openHistorySourceUrl(historyId: string): Promise<void>;
  openSupportPage(): Promise<void>;
  openContactEmail(): Promise<void>;
  checkForUpdates(): Promise<UpdateInfo | null>;
  downloadAndInstallUpdate(onProgress?: UpdateProgressCallback): Promise<void>;
  restartApp(): Promise<void>;
  checkEngineUpdate(): Promise<EngineUpdateStatusDto>;
  updateEngine(): Promise<EngineUpdateResultDto>;
  rollbackEngine(): Promise<EngineUpdateResultDto>;
  checkJsRuntime(): Promise<JsRuntimeStatusDto>;
  installJsRuntime(): Promise<JsRuntimeStatusDto>;
}

export class TauriIpcClient implements IpcClient {
  async healthCheck(): Promise<HealthStatus> {
    try {
      return await invoke<HealthStatus>('health_check');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async analyzeUrl(url: string): Promise<ProbeResult> {
    try {
      return await invoke<ProbeResult>('analyze_url', { request: { url } });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async cancelAnalyze(): Promise<void> {
    try {
      await invoke('cancel_analyze');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async detectPlaylist(url: string): Promise<PlaylistDetectionDto> {
    try {
      return await invoke<PlaylistDetectionDto>('detect_playlist', {
        request: { url },
      });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async cancelPlaylistDetection(): Promise<void> {
    try {
      await invoke('cancel_playlist_detection');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async getSettings(): Promise<AppSettingsDto> {
    try {
      return await invoke<AppSettingsDto>('get_settings');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async setSettings(settings: AppSettingsDto): Promise<AppSettingsDto> {
    try {
      return await invoke<AppSettingsDto>('set_settings', { request: { settings } });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async startDownload(
    url: string,
    preset?: DownloadPresetDto,
    outputDirectory?: string,
  ): Promise<DownloadJobDto> {
    try {
      return await invoke<DownloadJobDto>('start_download', {
        request: { url, preset, outputDirectory },
      });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async startPlaylistDownload(
    url: string,
    preset: DownloadPresetDto | undefined,
    outputDirectory: string | undefined,
    selectedUrls: string[],
  ): Promise<DownloadJobDto[]> {
    try {
      return await invoke<DownloadJobDto[]>('start_playlist_download', {
        request: { url, preset, outputDirectory, selectedUrls },
      });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async listDownloads(): Promise<DownloadJobDto[]> {
    try {
      return await invoke<DownloadJobDto[]>('list_downloads');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async cancelDownload(downloadId: string): Promise<DownloadJobDto> {
    try {
      return await invoke<DownloadJobDto>('cancel_download', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async dismissDownload(downloadId: string): Promise<void> {
    try {
      await invoke('dismiss_download', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async retryDownload(downloadId: string): Promise<DownloadJobDto> {
    try {
      return await invoke<DownloadJobDto>('retry_download', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openDownloadSourceUrl(downloadId: string): Promise<void> {
    try {
      await invoke('open_download_source_url', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async pickDirectory(defaultPath?: string): Promise<string | null> {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        canCreateDirectories: true,
        defaultPath: defaultPath && defaultPath.trim().length > 0 ? defaultPath : undefined,
      });

      if (selected === null || selected === undefined) {
        return null;
      }
      if (Array.isArray(selected)) {
        return selected[0] ?? null;
      }
      return selected;
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async revealDownloadedFile(downloadId: string): Promise<void> {
    try {
      await invoke('reveal_downloaded_file', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openDownloadedFile(downloadId: string): Promise<void> {
    try {
      await invoke('open_downloaded_file', { downloadId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async listDownloadHistory(): Promise<DownloadHistoryEntryDto[]> {
    try {
      return await invoke<DownloadHistoryEntryDto[]>('list_download_history');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async removeDownloadHistoryEntry(historyId: string): Promise<void> {
    try {
      await invoke('remove_download_history_entry', { historyId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async revealHistoryFile(historyId: string): Promise<void> {
    try {
      await invoke('reveal_history_file', { historyId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openHistoryFile(historyId: string): Promise<void> {
    try {
      await invoke('open_history_file', { historyId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openHistorySourceUrl(historyId: string): Promise<void> {
    try {
      await invoke('open_history_source_url', { historyId });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openSupportPage(): Promise<void> {
    try {
      await invoke('open_support_page');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async openContactEmail(): Promise<void> {
    try {
      await invoke('open_contact_email');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  private currentUpdate: Update | null = null;

  async checkForUpdates(): Promise<UpdateInfo | null> {
    try {
      const update = await check();
      if (!update) {
        this.currentUpdate = null;
        return null;
      }
      this.currentUpdate = update;
      return {
        version: update.version,
        currentVersion: update.currentVersion,
        body: update.body,
        date: update.date,
      };
    } catch (err) {
      this.currentUpdate = null;
      throw normalizeIpcError(err);
    }
  }

  async downloadAndInstallUpdate(onProgress?: UpdateProgressCallback): Promise<void> {
    if (!this.currentUpdate) {
      throw new Error('No update pending installation');
    }
    try {
      let downloaded = 0;
      let total: number | null = null;
      await this.currentUpdate.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? null;
          onProgress?.(downloaded, total);
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          onProgress?.(downloaded, total);
        } else if (event.event === 'Finished') {
          onProgress?.(total ?? downloaded, total);
        }
      });
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async restartApp(): Promise<void> {
    try {
      await relaunch();
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async checkEngineUpdate(): Promise<EngineUpdateStatusDto> {
    try {
      return await invoke<EngineUpdateStatusDto>('check_engine_update');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async updateEngine(): Promise<EngineUpdateResultDto> {
    try {
      return await invoke<EngineUpdateResultDto>('update_engine');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async rollbackEngine(): Promise<EngineUpdateResultDto> {
    try {
      return await invoke<EngineUpdateResultDto>('rollback_engine');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async checkJsRuntime(): Promise<JsRuntimeStatusDto> {
    try {
      return await invoke<JsRuntimeStatusDto>('check_js_runtime');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }

  async installJsRuntime(): Promise<JsRuntimeStatusDto> {
    try {
      return await invoke<JsRuntimeStatusDto>('install_js_runtime');
    } catch (err) {
      throw normalizeIpcError(err);
    }
  }
}

export const defaultIpcClient: IpcClient = new TauriIpcClient();

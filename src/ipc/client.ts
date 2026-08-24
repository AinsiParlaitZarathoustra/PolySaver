// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import type {
  AppError,
  AppSettingsDto,
  DownloadHistoryEntryDto,
  DownloadJobDto,
  DownloadPresetDto,
  HealthStatus,
  ProbeResult,
  UpdateInfo,
  UpdateProgressCallback,
} from './contracts';

/**
 * Normalizes any unknown rejection into a structured AppError.
 */
export function normalizeIpcError(err: unknown): AppError {
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    const candidate = err as { code: unknown; message: unknown };
    return {
      code: String(candidate.code),
      message: String(candidate.message),
    };
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
  getSettings(): Promise<AppSettingsDto>;
  setSettings(settings: AppSettingsDto): Promise<AppSettingsDto>;
  startDownload(
    url: string,
    preset?: DownloadPresetDto,
    outputDirectory?: string,
  ): Promise<DownloadJobDto>;
  listDownloads(): Promise<DownloadJobDto[]>;
  cancelDownload(downloadId: string): Promise<DownloadJobDto>;
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
  checkForUpdates(): Promise<UpdateInfo | null>;
  downloadAndInstallUpdate(onProgress?: UpdateProgressCallback): Promise<void>;
  restartApp(): Promise<void>;
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
}

export const defaultIpcClient: IpcClient = new TauriIpcClient();

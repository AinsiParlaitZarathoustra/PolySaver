// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { useState, useEffect, useRef, useCallback } from 'react';
import type { AppSettingsDto } from '../../ipc/contracts';
import { type IpcClient, defaultIpcClient } from '../../ipc/client';
import { setAppLanguage } from '../../i18n';

export type AutosaveStatus = 'idle' | 'saving' | 'saved' | 'error';

export interface UseAutosaveSettingsResult {
  settings: AppSettingsDto;
  updateSettings: (
    partial: Partial<AppSettingsDto> | ((prev: AppSettingsDto) => AppSettingsDto),
    immediate?: boolean,
  ) => void;
  status: AutosaveStatus;
  errorMessage: string | null;
  resetError: () => void;
}

const DEFAULT_SETTINGS: AppSettingsDto = {
  downloadDirectory: '',
  themeMode: 'system',
  parallelDownloads: true,
  defaultPreset: {
    format: 'mp4',
    videoQuality: 'p1080',
  },
  maxConcurrent: 3,
  language: 'fr',
};

export function useAutosaveSettings(
  client: IpcClient = defaultIpcClient,
  debounceMs: number = 400,
): UseAutosaveSettingsResult {
  const [settings, setSettings] = useState<AppSettingsDto>(DEFAULT_SETTINGS);
  const [status, setStatus] = useState<AutosaveStatus>('idle');
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // References for synchronization and concurrency management
  const lastPersistedRef = useRef<AppSettingsDto>(DEFAULT_SETTINGS);
  const latestSettingsRef = useRef<AppSettingsDto>(DEFAULT_SETTINGS);
  const isSavingRef = useRef<boolean>(false);
  const pendingSaveRef = useRef<AppSettingsDto | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savedIndicatorTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Load initial settings on mount
  useEffect(() => {
    let isMounted = true;

    const loadInitial = async () => {
      try {
        const loaded = await client.getSettings();
        if (isMounted) {
          const normalized = {
            ...loaded,
            language: loaded.language ?? 'fr',
          };
          setSettings(normalized);
          lastPersistedRef.current = normalized;
          latestSettingsRef.current = normalized;
          void setAppLanguage(normalized.language);
        }
      } catch {
        // Fallback to default settings
        void setAppLanguage('fr');
      }
    };

    void loadInitial();

    return () => {
      isMounted = false;
      if (timerRef.current) clearTimeout(timerRef.current);
      if (savedIndicatorTimerRef.current) clearTimeout(savedIndicatorTimerRef.current);
    };
  }, [client]);

  // Core execution loop for saving
  const executeSave = useCallback(
    async (snapshotToSave: AppSettingsDto) => {
      if (isSavingRef.current) {
        pendingSaveRef.current = snapshotToSave;
        return;
      }

      isSavingRef.current = true;
      setStatus('saving');
      setErrorMessage(null);

      try {
        const saved = await client.setSettings(snapshotToSave);
        const normalized = {
          ...saved,
          language: saved.language ?? 'fr',
        };
        lastPersistedRef.current = normalized;
        setStatus('saved');

        // Only update downloadDirectory in state if no newer input was typed in the meantime
        setSettings((current) => {
          if (current.downloadDirectory === snapshotToSave.downloadDirectory) {
            return {
              ...current,
              downloadDirectory: normalized.downloadDirectory,
            };
          }
          return current;
        });

        if (latestSettingsRef.current.downloadDirectory === snapshotToSave.downloadDirectory) {
          latestSettingsRef.current = {
            ...latestSettingsRef.current,
            downloadDirectory: normalized.downloadDirectory,
          };
        }

        // Reset 'saved' indicator after 2 seconds
        if (savedIndicatorTimerRef.current) {
          clearTimeout(savedIndicatorTimerRef.current);
        }
        savedIndicatorTimerRef.current = setTimeout(() => {
          setStatus('idle');
        }, 2000);
      } catch (err) {
        // Rollback to last known persisted state
        const fallback = lastPersistedRef.current;
        setSettings(fallback);
        latestSettingsRef.current = fallback;
        if (fallback.language) {
          void setAppLanguage(fallback.language);
        }
        setStatus('error');
        setErrorMessage(
          err instanceof Error ? err.message : "Échec de l'enregistrement automatique",
        );
      } finally {
        isSavingRef.current = false;

        // If another update arrived while writing, flush it now
        if (pendingSaveRef.current !== null) {
          const next = pendingSaveRef.current;
          pendingSaveRef.current = null;
          void executeSave(next);
        }
      }
    },
    [client],
  );

  const updateSettings = useCallback(
    (
      update: Partial<AppSettingsDto> | ((prev: AppSettingsDto) => AppSettingsDto),
      immediate: boolean = false,
    ) => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }

      const prev = latestSettingsRef.current;
      const updated = typeof update === 'function' ? update(prev) : { ...prev, ...update };
      latestSettingsRef.current = updated;
      setSettings(updated);

      if (updated.language && updated.language !== prev.language) {
        void setAppLanguage(updated.language);
      }

      if (immediate) {
        void executeSave(updated);
      } else {
        timerRef.current = setTimeout(() => {
          void executeSave(latestSettingsRef.current);
        }, debounceMs);
      }
    },
    [debounceMs, executeSave],
  );

  const resetError = useCallback(() => {
    setErrorMessage(null);
    setStatus('idle');
  }, []);

  return {
    settings,
    updateSettings,
    status,
    errorMessage,
    resetError,
  };
}

// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useEffect, useMemo, useCallback, useRef } from 'react';
import {
  ThemeProvider,
  CssBaseline,
  Container,
  Box,
  Alert,
  Snackbar,
} from '@mui/material';
import { useTranslation } from 'react-i18next';
import { createAppTheme, resolveThemeMode } from './theme';
import { Header } from '../components/Header';
import { DownloadForm } from '../components/DownloadForm';
import { DownloadQueue } from '../components/DownloadQueue';
import { DownloadHistory } from '../components/DownloadHistory';
import { SettingsDrawer } from '../components/SettingsDrawer';
import { DownloadOptionsDialog } from '../components/DownloadOptionsDialog';
import { HelpSupportDialog } from '../components/HelpSupportDialog';
import { UpdatePromptDialog } from '../components/UpdatePromptDialog';
import type {
  DownloadHistoryEntryDto,
  DownloadJobDto,
  DownloadPresetDto,
  DownloadProgressEvent,
  ProbeResult,
  UpdateInfo,
} from '../ipc/contracts';
import { defaultIpcClient } from '../ipc/client';
import {
  onDownloadCanceled,
  onDownloadCompleted,
  onDownloadFailed,
  onDownloadProgress,
  onDownloadQueued,
} from '../ipc/events';
import { useAutosaveSettings } from '../features/settings/useAutosaveSettings';

export const App: React.FC = () => {
  const { t } = useTranslation();
  const { settings, updateSettings, status: saveStatus, errorMessage: saveErrorMessage } =
    useAutosaveSettings(defaultIpcClient);

  const [jobs, setJobs] = useState<DownloadJobDto[]>([]);
  const [historyEntries, setHistoryEntries] = useState<DownloadHistoryEntryDto[]>([]);
  const [settingsOpen, setSettingsOpen] = useState<boolean>(false);
  const [helpOpen, setHelpOpen] = useState<boolean>(false);
  const [updatePromptOpen, setUpdatePromptOpen] = useState<boolean>(false);
  const [autoUpdateInfo, setAutoUpdateInfo] = useState<UpdateInfo | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  // Guided dialog state
  const [dialogOpen, setDialogOpen] = useState<boolean>(false);
  const [dialogUrl, setDialogUrl] = useState<string>('');
  const [probeResult, setProbeResult] = useState<ProbeResult | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState<boolean>(false);
  const [analyzeError, setAnalyzeError] = useState<string | null>(null);
  const [isSubmittingGuided, setIsSubmittingGuided] = useState<boolean>(false);

  // System OS theme listener
  const [systemScheme, setSystemScheme] = useState<'light' | 'dark'>(() =>
    resolveThemeMode('system'),
  );

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => {
      setSystemScheme(e.matches ? 'dark' : 'light');
    };
    mediaQuery.addEventListener('change', handler);
    return () => mediaQuery.removeEventListener('change', handler);
  }, []);

  const activeThemeMode = useMemo(() => {
    if (settings.themeMode === 'system') {
      return systemScheme;
    }
    return settings.themeMode;
  }, [settings.themeMode, systemScheme]);

  const theme = useMemo(
    () => createAppTheme(activeThemeMode),
    [activeThemeMode],
  );

  const tRef = useRef(t);
  useEffect(() => {
    tRef.current = t;
  }, [t]);

  // Load initial download jobs and history, and subscribe to real-time events once on mount
  useEffect(() => {
    let isMounted = true;
    const unlistenList: Array<() => void> = [];

    const init = async () => {
      try {
        const [initialJobs, initialHistory] = await Promise.all([
          defaultIpcClient.listDownloads(),
          defaultIpcClient.listDownloadHistory(),
        ]);
        if (!isMounted) return;
        // Filter out completed jobs from active queue to avoid duplicates with history
        setJobs(initialJobs.filter((j) => j.status !== 'completed'));
        setHistoryEntries(initialHistory);
      } catch {
        // Fallback to empty states
      }

      const uQueued = await onDownloadQueued((job) => {
        if (!isMounted) return;
        setJobs((prev) => {
          const idx = prev.findIndex((j) => j.id === job.id);
          if (idx >= 0) {
            const updated = [...prev];
            updated[idx] = job;
            return updated;
          }
          return [job, ...prev];
        });
      });
      if (isMounted) unlistenList.push(uQueued); else uQueued();

      const uProgress = await onDownloadProgress((event: DownloadProgressEvent) => {
        if (!isMounted) return;
        setJobs((prev) =>
          prev.map((job) => {
            if (job.id === event.downloadId) {
              return {
                ...job,
                status: event.phase,
                progressPercent: event.percent !== undefined && event.percent !== null ? event.percent : undefined,
                downloadedBytes: event.downloadedBytes !== undefined && event.downloadedBytes !== null ? event.downloadedBytes : undefined,
                totalBytes: event.totalBytes !== undefined && event.totalBytes !== null ? event.totalBytes : undefined,
                speedBytesPerSecond: event.speedBytesPerSecond !== undefined && event.speedBytesPerSecond !== null ? event.speedBytesPerSecond : undefined,
              };
            }
            return job;
          }),
        );
      });
      if (isMounted) unlistenList.push(uProgress); else uProgress();

      const uCompleted = await onDownloadCompleted((job) => {
        if (!isMounted) return;
        // Remove from active queue immediately
        setJobs((prev) => prev.filter((j) => j.id !== job.id));

        // Refresh persistent history from backend
        void (async () => {
          try {
            const updatedHistory = await defaultIpcClient.listDownloadHistory();
            if (isMounted) {
              setHistoryEntries(updatedHistory);
            }
          } catch {
            // Fallback: manually construct entry if backend list fails
            const fallbackEntry: DownloadHistoryEntryDto = {
              id: job.id,
              downloadId: job.id,
              sourceUrl: job.url,
              title: job.title || job.url,
              preset: job.preset,
              destinationPath: job.destinationPath || '',
              completedAt: Date.now(),
            };
            if (isMounted) {
              setHistoryEntries((prev) => [
                fallbackEntry,
                ...prev.filter((h) => h.downloadId !== job.id),
              ]);
            }
          }
        })();

        setToastMessage(`${tRef.current('queue.status.completed')}: ${job.title || job.url}`);
      });
      if (isMounted) unlistenList.push(uCompleted); else uCompleted();

      const uFailed = await onDownloadFailed((job) => {
        if (!isMounted) return;
        setJobs((prev) =>
          prev.map((j) => (j.id === job.id ? job : j)),
        );
        setToastMessage(`${tRef.current('queue.status.failed')}: ${job.errorMessage || tRef.current('errors.UNKNOWN_ERROR')}`);
      });
      if (isMounted) unlistenList.push(uFailed); else uFailed();

      const uCanceled = await onDownloadCanceled((job) => {
        if (!isMounted) return;
        setJobs((prev) =>
          prev.map((j) => (j.id === job.id ? job : j)),
        );
      });
      if (isMounted) unlistenList.push(uCanceled); else uCanceled();
    };

    void init();

    return () => {
      isMounted = false;
      for (const unlisten of unlistenList) {
        unlisten();
      }
    };
  }, []);

  // Automatic non-blocking check for updates on startup
  useEffect(() => {
    let isMounted = true;
    const checkUpdates = async () => {
      try {
        const update = await defaultIpcClient.checkForUpdates();
        if (isMounted && update) {
          setAutoUpdateInfo(update);
          setUpdatePromptOpen(true);
        }
      } catch {
        // Silent on startup network errors
      }
    };
    void checkUpdates();
    return () => {
      isMounted = false;
    };
  }, []);

  const hasActiveDownloads = useMemo(
    () =>
      jobs.some(
        (j) =>
          j.status === 'downloading' ||
          j.status === 'converting' ||
          j.status === 'preparing' ||
          j.status === 'probing',
      ),
    [jobs],
  );

  // 1. Fast Download: passes undefined preset and undefined output directory
  const handleFastDownload = useCallback(
    async (url: string): Promise<boolean> => {
      const job = await defaultIpcClient.startDownload(url, undefined, undefined);
      setJobs((prev) => {
        if (prev.some((j) => j.id === job.id)) return prev;
        return [job, ...prev];
      });
      return true;
    },
    [],
  );

  // 2. Guided Download: analyzes URL and opens DownloadOptionsDialog
  const handleGuidedDownload = useCallback(async (url: string) => {
    setDialogUrl(url);
    setProbeResult(null);
    setAnalyzeError(null);
    setIsAnalyzing(true);
    setDialogOpen(true);

    try {
      const probe = await defaultIpcClient.analyzeUrl(url);
      setProbeResult(probe);
    } catch (err) {
      setAnalyzeError(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    } finally {
      setIsAnalyzing(false);
    }
  }, [t]);

  // Confirmation from inside Guided Dialog
  const handleConfirmGuidedDownload = useCallback(
    async (chosenPreset: DownloadPresetDto, chosenOutputDirectory?: string) => {
      setIsSubmittingGuided(true);
      try {
        const outputDir =
          chosenOutputDirectory && chosenOutputDirectory !== settings.downloadDirectory
            ? chosenOutputDirectory
            : undefined;

        const job = await defaultIpcClient.startDownload(
          dialogUrl,
          chosenPreset,
          outputDir,
        );
        setJobs((prev) => {
          if (prev.some((j) => j.id === job.id)) return prev;
          return [job, ...prev];
        });
        setDialogOpen(false);
      } finally {
        setIsSubmittingGuided(false);
      }
    },
    [dialogUrl, settings.downloadDirectory],
  );

  // Cancel active job in pipeline
  const handleCancelJob = useCallback(async (jobId: string) => {
    try {
      const canceledJob = await defaultIpcClient.cancelDownload(jobId);
      setJobs((prev) =>
        prev.map((j) => (j.id === jobId ? canceledJob : j)),
      );
    } catch (err) {
      console.error('Failed to cancel job:', err);
    }
  }, []);

  // Dismiss job from active queue in memory
  const handleDismissJob = useCallback(async (jobId: string) => {
    try {
      await defaultIpcClient.dismissDownload(jobId);
      setJobs((prev) => prev.filter((j) => j.id !== jobId));
    } catch (err) {
      console.error('Failed to dismiss job:', err);
    }
  }, []);

  // Remove history entry
  const handleRemoveHistoryEntry = useCallback(
    async (historyId: string) => {
      const previous = historyEntries;
      setHistoryEntries((prev) => prev.filter((e) => e.id !== historyId));
      try {
        await defaultIpcClient.removeDownloadHistoryEntry(historyId);
      } catch (err) {
        console.error('Failed to remove history entry:', err);
        setHistoryEntries(previous);
        setToastMessage(t('errors.HISTORY_SAVE_FAILED'));
      }
    },
    [historyEntries, t],
  );

  const activeJobs = useMemo(
    () => jobs.filter((j) => j.status !== 'completed'),
    [jobs],
  );

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box
        sx={{
          minHeight: '100vh',
          display: 'flex',
          flexDirection: 'column',
          bgcolor: 'background.default',
        }}
      >
        <Header
          onOpenSettings={() => setSettingsOpen(true)}
          onOpenHelp={() => setHelpOpen(true)}
        />

        <Container maxWidth="md" sx={{ py: { xs: 3, sm: 4 }, flexGrow: 1 }}>
          <DownloadForm
            onFastDownload={handleFastDownload}
            onGuidedDownload={handleGuidedDownload}
            isProcessing={isAnalyzing}
          />

          <DownloadQueue
            jobs={activeJobs}
            onDismissJob={handleDismissJob}
            onCancelJob={handleCancelJob}
            hideEmptyQueue={historyEntries.length > 0 && activeJobs.length === 0}
          />

          <DownloadHistory
            entries={historyEntries}
            onRemoveEntry={handleRemoveHistoryEntry}
          />
        </Container>

        {/* Guided Download Options Dialog */}
        <DownloadOptionsDialog
          open={dialogOpen}
          onClose={() => setDialogOpen(false)}
          probeResult={probeResult}
          isLoading={isAnalyzing}
          errorMessage={analyzeError}
          defaultPreset={settings.defaultPreset}
          defaultDownloadDirectory={settings.downloadDirectory}
          onConfirmDownload={handleConfirmGuidedDownload}
          isSubmitting={isSubmittingGuided}
        />

        {/* Help & Support Dialog */}
        <HelpSupportDialog
          open={helpOpen}
          onClose={() => setHelpOpen(false)}
          client={defaultIpcClient}
          hasActiveDownloads={hasActiveDownloads}
        />

        {/* Software Update Startup Prompt Dialog */}
        <UpdatePromptDialog
          open={updatePromptOpen}
          onClose={() => setUpdatePromptOpen(false)}
          updateInfo={autoUpdateInfo}
          client={defaultIpcClient}
          hasActiveDownloads={hasActiveDownloads}
        />

        {/* Settings Drawer with Autosave and Folder Browse */}
        <SettingsDrawer
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          settings={settings}
          onUpdateSettings={updateSettings}
          saveStatus={saveStatus}
          errorMessage={saveErrorMessage}
          onBrowseDirectory={(path) => defaultIpcClient.pickDirectory(path)}
        />

        <Snackbar
          open={Boolean(toastMessage)}
          autoHideDuration={4000}
          onClose={() => setToastMessage(null)}
          anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
        >
          <Alert
            onClose={() => setToastMessage(null)}
            severity={toastMessage?.includes(t('queue.status.failed')) ? 'error' : 'success'}
            sx={{ width: '100%', borderRadius: 2 }}
          >
            {toastMessage}
          </Alert>
        </Snackbar>
      </Box>
    </ThemeProvider>
  );
};

export default App;

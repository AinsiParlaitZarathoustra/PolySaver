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
  Button,
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
import { JsRuntimeSetupDialog } from '../components/JsRuntimeSetupDialog';
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
  onDownloadWarning,
} from '../ipc/events';
import { useAutosaveSettings } from '../features/settings/useAutosaveSettings';

/**
 * Localized failure message for a job: the i18n key for its error code when it
 * exists, otherwise the backend message or a generic fallback.
 */
function formatJobError(
  job: DownloadJobDto,
  translate: (key: string) => string,
): string {
  const code = job.errorDetails?.code;
  if (code) {
    const key = `errors.${code}`;
    const localized = translate(key);
    if (localized && localized !== key) {
      return localized;
    }
  }
  return job.errorMessage || translate('errors.UNKNOWN_ERROR');
}

/** Adapter so the strongly-typed i18next `t` can be passed where a loose lookup is expected. */
function asLookup(translate: (key: never) => string): (key: string) => string {
  return (key: string) => translate(key as never);
}

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
  const [toastSeverity, setToastSeverity] = useState<'success' | 'error' | 'warning'>('success');
  const [warningToast, setWarningToast] = useState<{ code: string; message: string } | null>(null);
  const [isUpdatingEngine, setIsUpdatingEngine] = useState<boolean>(false);
  const [jsRuntimeDialogOpen, setJsRuntimeDialogOpen] = useState<boolean>(false);
  // The setup prompt is offered at most once per session.
  const jsRuntimePromptedRef = useRef<boolean>(false);

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
        // Insert unknown jobs: a `failed` event for a job never received must not be lost.
        setJobs((prev) =>
          prev.some((j) => j.id === job.id)
            ? prev.map((j) => (j.id === job.id ? job : j))
            : [job, ...prev],
        );
        setToastSeverity('error');
        setToastMessage(
          `${tRef.current('queue.status.failed')}: ${formatJobError(job, asLookup(tRef.current))}`,
        );
      });
      if (isMounted) unlistenList.push(uFailed); else uFailed();

      const uCanceled = await onDownloadCanceled((job) => {
        if (!isMounted) return;
        // Same insertion rule as `failed`, so a canceled event is never dropped.
        setJobs((prev) =>
          prev.some((j) => j.id === job.id)
            ? prev.map((j) => (j.id === job.id ? job : j))
            : [job, ...prev],
        );
      });
      if (isMounted) unlistenList.push(uCanceled); else uCanceled();

      // Non-fatal backend warnings (e.g. outdated engine on a successful download)
      const uWarning = await onDownloadWarning((warning) => {
        if (!isMounted) return;
        setWarningToast({ code: warning.code, message: warning.message });
      });
      if (isMounted) unlistenList.push(uWarning); else uWarning();
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

  // Offer the JavaScript runtime once per session when none is usable.
  // yt-dlp needs it for the full YouTube format list ("n challenge" solving).
  useEffect(() => {
    let isMounted = true;
    const checkRuntime = async () => {
      if (jsRuntimePromptedRef.current) return;
      try {
        const status = await defaultIpcClient.checkJsRuntime();
        if (!isMounted) return;
        if (!status.isReady) {
          jsRuntimePromptedRef.current = true;
          setJsRuntimeDialogOpen(true);
        }
      } catch {
        // Detection failure must never block startup.
      }
    };
    void checkRuntime();
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
  const analyzeGenerationRef = useRef<number>(0);

  const handleGuidedDownload = useCallback(async (url: string) => {
    const generation = analyzeGenerationRef.current + 1;
    analyzeGenerationRef.current = generation;

    setDialogUrl(url);
    setProbeResult(null);
    setAnalyzeError(null);
    setIsAnalyzing(true);
    setDialogOpen(true);

    try {
      const probe = await defaultIpcClient.analyzeUrl(url);
      // Ignore responses belonging to a superseded or canceled analysis generation.
      if (analyzeGenerationRef.current !== generation) return;
      setProbeResult(probe);
    } catch (err) {
      if (analyzeGenerationRef.current !== generation) return;
      setAnalyzeError(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    } finally {
      if (analyzeGenerationRef.current === generation) {
        setIsAnalyzing(false);
      }
    }
  }, [t]);

  // Close the guided dialog and immediately abort any in-flight analysis process.
  const handleCancelAnalyze = useCallback(async () => {
    // Invalidate the current generation so a late response cannot repopulate state.
    analyzeGenerationRef.current += 1;
    setDialogOpen(false);
    setIsAnalyzing(false);
    setProbeResult(null);
    setAnalyzeError(null);
    try {
      await defaultIpcClient.cancelAnalyze();
    } catch (err) {
      console.error('Failed to cancel analysis:', err);
      setToastSeverity('error');
      setToastMessage(t('errors.DOWNLOAD_PROCESS_FAILED'));
    }
  }, [t]);

  // Confirmation from inside Guided Dialog
  const handleConfirmGuidedDownload = useCallback(
    async (
      chosenPreset: DownloadPresetDto,
      chosenOutputDirectory?: string,
      selectedUrls?: string[],
    ) => {
      setIsSubmittingGuided(true);
      try {
        const outputDir =
          chosenOutputDirectory && chosenOutputDirectory !== settings.downloadDirectory
            ? chosenOutputDirectory
            : undefined;

        // Playlist mode: one job per selected video, so each keeps its own progress,
        // cancellation, retry and history entry. A failure here must never be silent.
        if (selectedUrls && selectedUrls.length > 0) {
          try {
            const queued = await defaultIpcClient.startPlaylistDownload(
              dialogUrl,
              chosenPreset,
              outputDir,
              selectedUrls,
            );
            setJobs((prev) => {
              const known = new Set(prev.map((j) => j.id));
              const additions = queued.filter((j) => !known.has(j.id));
              return [...additions, ...prev];
            });
            setToastSeverity('success');
            setToastMessage(t('queue.playlistQueued', { count: queued.length }));
            setDialogOpen(false);
          } catch (err) {
            setToastSeverity('error');
            setToastMessage(
              err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
            );
          }
          return;
        }

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
    [dialogUrl, settings.downloadDirectory, t],
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
      setToastSeverity('error');
      setToastMessage(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    }
  }, [t]);

  // Dismiss job from active queue in memory
  const handleDismissJob = useCallback(async (jobId: string) => {
    try {
      await defaultIpcClient.dismissDownload(jobId);
      setJobs((prev) => prev.filter((j) => j.id !== jobId));
    } catch (err) {
      console.error('Failed to dismiss job:', err);
      setToastSeverity('error');
      setToastMessage(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    }
  }, [t]);

  // Re-queue a failed job as a brand new entry (URL, preset and directory preserved)
  const handleRetryJob = useCallback(
    async (jobId: string) => {
      try {
        const newJob = await defaultIpcClient.retryDownload(jobId);
        setJobs((prev) => {
          const withoutOld = prev.filter((j) => j.id !== jobId);
          if (withoutOld.some((j) => j.id === newJob.id)) return withoutOld;
          return [newJob, ...withoutOld];
        });
      } catch (err) {
        setToastSeverity('error');
        setToastMessage(
          err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
        );
      }
    },
    [t],
  );

  // Download and install a newer yt-dlp engine, then refresh the diagnostics
  const handleUpdateEngine = useCallback(async () => {
    setIsUpdatingEngine(true);
    setWarningToast(null);
    try {
      await defaultIpcClient.updateEngine();
      setToastSeverity('success');
      setToastMessage(t('engine.upToDate'));
    } catch (err) {
      setToastSeverity('error');
      setToastMessage(
        err instanceof Error ? err.message : t('engine.updateFailed'),
      );
    } finally {
      setIsUpdatingEngine(false);
    }
  }, [t]);

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

  // Surface a component-level failure to the user (never console-only).
  const handleComponentError = useCallback((message: string) => {
    setToastSeverity('error');
    setToastMessage(message);
  }, []);

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
            onRetryJob={handleRetryJob}
            onUpdateEngine={handleUpdateEngine}
            isUpdatingEngine={isUpdatingEngine}
            onError={handleComponentError}
            hideEmptyQueue={historyEntries.length > 0 && activeJobs.length === 0}
          />

          <DownloadHistory
            entries={historyEntries}
            onRemoveEntry={handleRemoveHistoryEntry}
            onError={handleComponentError}
          />
        </Container>

        {/* Guided Download Options Dialog */}
        <DownloadOptionsDialog
          open={dialogOpen}
          onClose={() => {
            void handleCancelAnalyze();
          }}
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
          onOpenPreferences={() => {
            setHelpOpen(false);
            setSettingsOpen(true);
          }}
          onError={handleComponentError}
        />

        {/* Software Update Startup Prompt Dialog */}
        <UpdatePromptDialog
          open={updatePromptOpen}
          onClose={() => setUpdatePromptOpen(false)}
          updateInfo={autoUpdateInfo}
          client={defaultIpcClient}
          hasActiveDownloads={hasActiveDownloads}
        />

        {/* JavaScript runtime first-launch setup (Deno/Node) */}
        <JsRuntimeSetupDialog
          open={jsRuntimeDialogOpen}
          onClose={() => setJsRuntimeDialogOpen(false)}
          client={defaultIpcClient}
          hasActiveDownloads={hasActiveDownloads}
          onInstalled={() => {
            setToastSeverity('success');
            setToastMessage(t('jsRuntime.installed'));
          }}
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
          onOpenJsRuntimeSetup={() => setJsRuntimeDialogOpen(true)}
        />

        <Snackbar
          open={Boolean(toastMessage)}
          autoHideDuration={4000}
          onClose={() => setToastMessage(null)}
          anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
        >
          <Alert
            onClose={() => setToastMessage(null)}
            severity={toastSeverity}
            sx={{ width: '100%', borderRadius: 2 }}
          >
            {toastMessage}
          </Alert>
        </Snackbar>

        {/* Non-fatal warnings (e.g. outdated engine reported during a successful download) */}
        <Snackbar
          open={Boolean(warningToast)}
          autoHideDuration={8000}
          onClose={() => setWarningToast(null)}
          anchorOrigin={{ vertical: 'bottom', horizontal: 'center' }}
        >
          <Alert
            severity="warning"
            onClose={() => setWarningToast(null)}
            action={
              <Button
                color="inherit"
                size="small"
                onClick={() => {
                  void handleUpdateEngine();
                }}
                disabled={isUpdatingEngine}
                sx={{ textTransform: 'none', fontWeight: 600 }}
              >
                {isUpdatingEngine ? t('engine.updating') : t('engine.updateButton')}
              </Button>
            }
            sx={{ width: '100%', borderRadius: 2 }}
          >
            {warningToast?.code === 'YTDLP_UPDATE_REQUIRED'
              ? t('engine.outdatedMessage')
              : warningToast?.message}
          </Alert>
        </Snackbar>
      </Box>
    </ThemeProvider>
  );
};

export default App;

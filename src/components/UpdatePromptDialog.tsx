// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useCallback, useEffect } from 'react';
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Typography,
  Button,
  Box,
  LinearProgress,
  Alert,
  CircularProgress,
} from '@mui/material';
import SystemUpdateAltIcon from '@mui/icons-material/SystemUpdateAlt';
import { useTranslation } from 'react-i18next';
import type { IpcClient } from '../ipc/client';
import type { UpdateInfo } from '../ipc/contracts';
import { extractWhatsNew, type WhatsNew } from '../utils/releaseNotes';
import { ReleaseNotes } from './ReleaseNotes';

interface UpdatePromptDialogProps {
  open: boolean;
  onClose: () => void;
  updateInfo: UpdateInfo | null;
  client: IpcClient;
  hasActiveDownloads: boolean;
}

export const UpdatePromptDialog: React.FC<UpdatePromptDialogProps> = ({
  open,
  onClose,
  updateInfo,
  client,
  hasActiveDownloads,
}) => {
  const { t } = useTranslation();
  const [isDownloading, setIsDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [isReadyToRestart, setIsReadyToRestart] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [releaseName, setReleaseName] = useState<string | null>(null);
  const [whatsNew, setWhatsNew] = useState<WhatsNew | null>(null);
  const [notesLoading, setNotesLoading] = useState(false);

  const version = updateInfo?.version;

  // The release page holds the full "what's new" text (the updater manifest
  // only carries a one-line note). Without it, the dialog still works.
  useEffect(() => {
    if (!open || !version) return;
    let active = true;
    setNotesLoading(true);
    client
      .getReleaseNotes(version)
      .then((notes) => {
        if (!active) return;
        setReleaseName(notes.name ?? null);
        setWhatsNew(extractWhatsNew(notes.body));
      })
      .catch(() => {
        if (active) setWhatsNew(null);
      })
      .finally(() => {
        if (active) setNotesLoading(false);
      });
    return () => {
      active = false;
    };
  }, [open, version, client]);

  const handleInstall = useCallback(async () => {
    if (hasActiveDownloads) return;
    setIsDownloading(true);
    setError(null);
    setDownloadProgress(0);
    try {
      await client.downloadAndInstallUpdate((downloaded, total) => {
        if (total && total > 0) {
          setDownloadProgress(Math.round((downloaded / total) * 100));
        } else {
          setDownloadProgress(null);
        }
      });
      setIsDownloading(false);
      setIsReadyToRestart(true);
    } catch (err) {
      console.error('Failed to download update:', err);
      setIsDownloading(false);
      setError(err instanceof Error ? err.message : t('help.updater.error'));
    }
  }, [client, hasActiveDownloads, t]);

  const handleRestart = useCallback(async () => {
    try {
      await client.restartApp();
    } catch (err) {
      console.error('Failed to restart app:', err);
      setError(err instanceof Error ? err.message : t('help.updater.error'));
    }
  }, [client, t]);

  if (!updateInfo) return null;

  const title = whatsNew?.title || releaseName || `PolySaver ${updateInfo.version}`;
  const hasNotes = Boolean(whatsNew && whatsNew.blocks.length > 0);

  return (
    <Dialog
      open={open}
      onClose={isDownloading ? undefined : onClose}
      maxWidth="sm"
      fullWidth
      aria-labelledby="update-prompt-dialog-title"
      PaperProps={{
        sx: {
          borderRadius: 3,
          p: 1,
        },
      }}
    >
      <DialogTitle
        id="update-prompt-dialog-title"
        sx={{ display: 'flex', alignItems: 'center', gap: 1.5, pb: 1 }}
      >
        <SystemUpdateAltIcon color="success" />
        <Typography variant="h6" component="h2" sx={{ fontWeight: 700 }}>
          {title}
        </Typography>
      </DialogTitle>

      <DialogContent sx={{ pb: 1 }}>
        {notesLoading ? (
          <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, my: 2 }}>
            <CircularProgress size={18} />
            <Typography variant="body2" color="text.secondary">
              {t('help.updater.notesLoading')}
            </Typography>
          </Box>
        ) : hasNotes && whatsNew ? (
          <Box sx={{ mb: 2 }}>
            <ReleaseNotes blocks={whatsNew.blocks} />
          </Box>
        ) : (
          <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
            {t('help.updater.updateAvailablePrompt')}
          </Typography>
        )}

        {hasActiveDownloads && !isReadyToRestart && (
          <Alert severity="warning" sx={{ mb: 2 }}>
            {t('help.updater.activeDownloadsWarning')}
          </Alert>
        )}

        {error && (
          <Alert severity="error" sx={{ mb: 2 }}>
            {error}
          </Alert>
        )}

        {isDownloading && (
          <Box sx={{ my: 1 }}>
            <Typography variant="caption" sx={{ fontWeight: 600, display: 'block', mb: 0.5 }}>
              {t('help.updater.downloadingUpdate')}
            </Typography>
            <LinearProgress
              variant={downloadProgress !== null ? 'determinate' : 'indeterminate'}
              value={downloadProgress ?? 0}
              sx={{ borderRadius: 1, height: 8 }}
            />
            {downloadProgress !== null && (
              <Typography variant="caption" color="text.secondary" sx={{ mt: 0.5, display: 'block' }}>
                {downloadProgress}%
              </Typography>
            )}
          </Box>
        )}

        {isReadyToRestart && (
          <Alert severity="success" sx={{ my: 1 }}>
            {t('help.updater.updateInstalled')}
          </Alert>
        )}
      </DialogContent>

      <DialogActions sx={{ px: 3, pb: 2 }}>
        {!isReadyToRestart ? (
          <>
            <Button onClick={onClose} disabled={isDownloading} color="inherit">
              {t('help.updater.updateLater')}
            </Button>
            <Button
              variant="contained"
              onClick={handleInstall}
              disabled={isDownloading || hasActiveDownloads}
            >
              {t('help.updater.updateNow')}
            </Button>
          </>
        ) : (
          <Button variant="contained" color="success" onClick={handleRestart} fullWidth>
            {t('help.updater.restartNow')}
          </Button>
        )}
      </DialogActions>
    </Dialog>
  );
};

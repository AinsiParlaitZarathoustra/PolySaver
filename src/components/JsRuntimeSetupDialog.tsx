// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useCallback, useState } from 'react';
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
} from '@mui/material';
import JavascriptIcon from '@mui/icons-material/Javascript';
import { useTranslation } from 'react-i18next';
import type { IpcClient } from '../ipc/client';
import type { JsRuntimeStatusDto } from '../ipc/contracts';

interface JsRuntimeSetupDialogProps {
  open: boolean;
  onClose: () => void;
  client: IpcClient;
  hasActiveDownloads: boolean;
  /** Called with the installed runtime after a successful installation. */
  onInstalled?: (status: JsRuntimeStatusDto) => void;
}

/**
 * First-launch prompt offering to install Node.js.
 *
 * Non-blocking by design: the user can dismiss it and still download (with a
 * degraded format list for YouTube). The primary action is auto-focused so
 * pressing Enter installs, as requested.
 */
export const JsRuntimeSetupDialog: React.FC<JsRuntimeSetupDialogProps> = ({
  open,
  onClose,
  client,
  hasActiveDownloads,
  onInstalled,
}) => {
  const { t } = useTranslation();
  const [isInstalling, setIsInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installedVersion, setInstalledVersion] = useState<string | null>(null);

  const handleInstall = useCallback(async () => {
    setIsInstalling(true);
    setError(null);
    try {
      const status = await client.installJsRuntime();
      setInstalledVersion(status.version ?? '');
      onInstalled?.(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('jsRuntime.installFailed'));
    } finally {
      setIsInstalling(false);
    }
  }, [client, onInstalled, t]);

  const handleClose = useCallback(() => {
    if (isInstalling) return;
    setError(null);
    setInstalledVersion(null);
    onClose();
  }, [isInstalling, onClose]);

  return (
    <Dialog
      open={open}
      onClose={isInstalling ? undefined : handleClose}
      maxWidth="xs"
      fullWidth
      aria-labelledby="js-runtime-dialog-title"
    >
      <DialogTitle
        id="js-runtime-dialog-title"
        sx={{ display: 'flex', alignItems: 'center', gap: 1.5, pb: 1 }}
      >
        <JavascriptIcon color="primary" />
        <Typography variant="h6" component="span" sx={{ fontWeight: 700 }}>
          {t('jsRuntime.title')}
        </Typography>
      </DialogTitle>

      <DialogContent>
        <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
          {t('jsRuntime.description')}
        </Typography>

        {isInstalling && (
          <Box sx={{ width: '100%', mt: 1 }}>
            <LinearProgress />
            <Typography variant="caption" color="text.secondary" sx={{ mt: 0.75, display: 'block' }}>
              {t('jsRuntime.installing')}
            </Typography>
          </Box>
        )}

        {installedVersion && (
          <Alert severity="success">
            {t('jsRuntime.installed')}
            {installedVersion
              ? ` — ${t('jsRuntime.currentVersion')}: ${installedVersion}`
              : ''}
          </Alert>
        )}

        {error && (
          <Alert severity="error" onClose={() => setError(null)}>
            {error}
          </Alert>
        )}

        {hasActiveDownloads && !isInstalling && !installedVersion && (
          <Alert severity="warning" sx={{ mt: 1 }}>
            {t('help.updater.activeDownloadsWarning')}
          </Alert>
        )}
      </DialogContent>

      <DialogActions sx={{ px: 3, pb: 2 }}>
        <Button color="inherit" onClick={handleClose} disabled={isInstalling}>
          {t('jsRuntime.laterButton')}
        </Button>
        {installedVersion ? (
          <Button variant="contained" onClick={handleClose}>
            {t('common.close')}
          </Button>
        ) : (
          <Button
            variant="contained"
            onClick={handleInstall}
            disabled={isInstalling || hasActiveDownloads}
            // Default keyboard action: Enter installs the runtime.
            autoFocus
          >
            {isInstalling ? t('jsRuntime.installing') : t('jsRuntime.installButton')}
          </Button>
        )}
      </DialogActions>
    </Dialog>
  );
};

export default JsRuntimeSetupDialog;

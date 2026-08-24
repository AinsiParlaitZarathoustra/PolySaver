// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState } from 'react';
import {
  Card,
  CardContent,
  TextField,
  Button,
  Box,
  CircularProgress,
  Alert,
  Stack,
  InputAdornment,
  Tooltip,
} from '@mui/material';
import BoltIcon from '@mui/icons-material/Bolt';
import DownloadIcon from '@mui/icons-material/Download';
import LinkIcon from '@mui/icons-material/Link';
import { useTranslation } from 'react-i18next';

interface DownloadFormProps {
  onFastDownload: (url: string) => Promise<boolean>;
  onGuidedDownload: (url: string) => void;
  isProcessing?: boolean;
}

export const DownloadForm: React.FC<DownloadFormProps> = ({
  onFastDownload,
  onGuidedDownload,
  isProcessing = false,
}) => {
  const { t } = useTranslation();
  const [url, setUrl] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [isFastDownloading, setIsFastDownloading] = useState(false);

  const validateUrl = (raw: string): string | null => {
    const trimmed = raw.trim();
    if (!trimmed) {
      setError(t('form.emptyUrlError'));
      return null;
    }

    if (!trimmed.startsWith('http://') && !trimmed.startsWith('https://')) {
      setError(t('form.invalidUrlError'));
      return null;
    }

    setError(null);
    return trimmed;
  };

  const handleFastDownload = async () => {
    const validUrl = validateUrl(url);
    if (!validUrl || isProcessing || isFastDownloading) return;

    setIsFastDownloading(true);
    try {
      const success = await onFastDownload(validUrl);
      if (success) {
        setUrl('');
      }
    } catch (err) {
      setError(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    } finally {
      setIsFastDownloading(false);
    }
  };

  const handleGuidedDownload = () => {
    const validUrl = validateUrl(url);
    if (!validUrl || isProcessing || isFastDownloading) return;

    onGuidedDownload(validUrl);
  };

  const isDisabled = isProcessing || isFastDownloading;

  return (
    <Card
      elevation={0}
      sx={{
        p: { xs: 2, sm: 3 },
        background: (theme) =>
          theme.palette.mode === 'dark'
            ? 'linear-gradient(145deg, #131b2e 0%, #0d1322 100%)'
            : 'linear-gradient(145deg, #ffffff 0%, #f8fafc 100%)',
        border: 1,
        borderColor: 'divider',
        boxShadow: (theme) =>
          theme.palette.mode === 'dark'
            ? '0 8px 32px 0 rgba(0, 0, 0, 0.37)'
            : '0 8px 32px 0 rgba(0, 0, 0, 0.06)',
      }}
    >
      <CardContent sx={{ p: '0 !important' }}>
        <Stack spacing={2.5}>
          {/* Streamlined URL Input */}
          <TextField
            fullWidth
            variant="outlined"
            placeholder={t('form.urlPlaceholder')}
            value={url}
            onChange={(e) => {
              setUrl(e.target.value);
              if (error) setError(null);
            }}
            disabled={isDisabled}
            InputProps={{
              startAdornment: (
                <InputAdornment position="start">
                  <LinkIcon color="action" />
                </InputAdornment>
              ),
            }}
            sx={{
              '& .MuiOutlinedInput-root': {
                bgcolor: (theme) =>
                  theme.palette.mode === 'dark'
                    ? 'rgba(0, 0, 0, 0.2)'
                    : 'rgba(255, 255, 255, 0.8)',
              },
            }}
          />

          {/* Action Buttons: Fast (Green) & Guided (Blue) */}
          <Box
            sx={{
              display: 'flex',
              flexDirection: { xs: 'column', sm: 'row' },
              gap: 2,
              justifyContent: 'flex-end',
            }}
          >
            {/* Fast Download (Green with Bolt) */}
            <Tooltip title={t('form.quickDownloadTooltip')}>
              <span>
                <Button
                  variant="contained"
                  color="success"
                  size="large"
                  onClick={handleFastDownload}
                  disabled={isDisabled || !url.trim()}
                  startIcon={
                    isFastDownloading ? (
                      <CircularProgress size={20} color="inherit" />
                    ) : (
                      <BoltIcon />
                    )
                  }
                  sx={{
                    width: { xs: '100%', sm: 'auto' },
                    py: 1.5,
                    px: 3,
                    fontWeight: 700,
                    fontSize: '0.95rem',
                    borderRadius: '10px',
                  }}
                >
                  {t('form.quickDownloadButton')}
                </Button>
              </span>
            </Tooltip>

            {/* Guided Download (Blue with Download) */}
            <Tooltip title={t('form.downloadTooltip')}>
              <span>
                <Button
                  variant="contained"
                  color="primary"
                  size="large"
                  onClick={handleGuidedDownload}
                  disabled={isDisabled || !url.trim()}
                  startIcon={
                    isProcessing && !isFastDownloading ? (
                      <CircularProgress size={20} color="inherit" />
                    ) : (
                      <DownloadIcon />
                    )
                  }
                  sx={{
                    width: { xs: '100%', sm: 'auto' },
                    py: 1.5,
                    px: 3,
                    fontWeight: 700,
                    fontSize: '0.95rem',
                    borderRadius: '10px',
                  }}
                >
                  {t('form.downloadButton')}
                </Button>
              </span>
            </Tooltip>
          </Box>

          {error && (
            <Alert severity="error" onClose={() => setError(null)}>
              {error}
            </Alert>
          )}
        </Stack>
      </CardContent>
    </Card>
  );
};

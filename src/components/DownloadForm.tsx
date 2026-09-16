// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useEffect, useRef } from 'react';
import {
  Card,
  CardContent,
  TextField,
  Button,
  Box,
  Chip,
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
import { defaultIpcClient } from '../ipc/client';

interface DownloadFormProps {
  onFastDownload: (url: string) => Promise<boolean>;
  onGuidedDownload: (url: string) => void;
  isProcessing?: boolean;
}

/** Delay before asking the engine, so typing does not trigger a request per key. */
const DETECT_DEBOUNCE_MS = 500;

/**
 * Fast URL-shape guess, used only as instant feedback while the engine answer is
 * pending (and as a fallback if the engine cannot be reached).
 *
 * The authoritative answer comes from the engine through `detectPlaylist`: only
 * yt-dlp knows what an URL really resolves to.
 *
 * A listing is a `/playlist`, `/channel`, `/c/`, `/user/` or `/@handle` path, or a
 * bare `list=` parameter — which is a YouTube convention (elsewhere `list` is an
 * ordinary query parameter such as pagination). `watch?v=X&list=Y` is a single
 * video, because the backend strips the share parameter.
 */
export const isPlaylistLikeUrl = (raw: string): boolean => {
  const trimmed = raw.trim();
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return false;
  }

  const path = parsed.pathname.toLowerCase();
  if (
    path.startsWith('/playlist') ||
    path.startsWith('/channel') ||
    path.startsWith('/c/') ||
    path.startsWith('/user/') ||
    path.startsWith('/@')
  ) {
    return true;
  }

  const host = parsed.hostname.toLowerCase();
  const isYouTube =
    host === 'youtube.com' ||
    host.endsWith('.youtube.com') ||
    host === 'youtu.be' ||
    host === 'youtube-nocookie.com' ||
    host.endsWith('.youtube-nocookie.com');
  if (!isYouTube) {
    return false;
  }

  const hasList = parsed.searchParams.has('list');
  const hasVideoId = parsed.searchParams.has('v');
  return hasList && !hasVideoId;
};

export const DownloadForm: React.FC<DownloadFormProps> = ({
  onFastDownload,
  onGuidedDownload,
  isProcessing = false,
}) => {
  const { t } = useTranslation();
  const [url, setUrl] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [isFastDownloading, setIsFastDownloading] = useState(false);
  // Instant shape guess; replaced by the engine answer as soon as it lands.
  const [isPlaylistUrl, setIsPlaylistUrl] = useState(false);
  // Generation counter: only the newest detection may write state.
  const detectGenerationRef = useRef(0);

  useEffect(() => {
    const trimmed = url.trim();
    if (!trimmed.startsWith('http://') && !trimmed.startsWith('https://')) {
      setIsPlaylistUrl(false);
      return undefined;
    }

    detectGenerationRef.current += 1;
    const generation = detectGenerationRef.current;

    // Instant feedback from the URL shape while the engine is asked.
    setIsPlaylistUrl(isPlaylistLikeUrl(trimmed));
    // Only cancel on cleanup when a request was actually sent: before the
    // debounce fires there is nothing running to stop.
    let requestSent = false;

    const timer = setTimeout(() => {
      requestSent = true;
      void (async () => {
        try {
          const detection = await defaultIpcClient.detectPlaylist(trimmed);
          if (detectGenerationRef.current !== generation) return;
          setIsPlaylistUrl(detection.isPlaylist);
        } catch {
          // Engine unreachable or URL not resolvable: keep the shape-based guess
          // rather than showing a stale answer for another URL.
        }
      })();
    }, DETECT_DEBOUNCE_MS);

    return () => {
      clearTimeout(timer);
      if (requestSent) {
        void defaultIpcClient.cancelPlaylistDetection().catch(() => {
          // Nothing left to cancel, or the bridge is unavailable (tests).
        });
      }
    };
  }, [url]);

  const playlistMode = isPlaylistUrl;

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
              alignItems: { xs: 'stretch', sm: 'center' },
              justifyContent: 'flex-end',
            }}
          >
            {playlistMode && (
              <Chip
                label={t('form.playlistModeBadge')}
                size="small"
                color="primary"
                variant="outlined"
                sx={{ alignSelf: { xs: 'flex-start', sm: 'center' } }}
              />
            )}

            {/* Fast Download (Green with Bolt) */}
            <Tooltip
              title={
                playlistMode ? t('form.playlistFastUnavailable') : t('form.quickDownloadTooltip')
              }
            >
              <span>
                <Button
                  variant="contained"
                  color="success"
                  size="large"
                  onClick={handleFastDownload}
                  disabled={isDisabled || !url.trim() || playlistMode}
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

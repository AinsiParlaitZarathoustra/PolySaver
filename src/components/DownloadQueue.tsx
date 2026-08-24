// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React from 'react';
import {
  Box,
  Card,
  CardContent,
  Typography,
  LinearProgress,
  Chip,
  Stack,
  Alert,
  Tooltip,
  IconButton,
} from '@mui/material';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import ErrorIcon from '@mui/icons-material/Error';
import HourglassEmptyIcon from '@mui/icons-material/HourglassEmpty';
import MovieIcon from '@mui/icons-material/Movie';
import MusicNoteIcon from '@mui/icons-material/MusicNote';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import CloseIcon from '@mui/icons-material/Close';
import { useTranslation } from 'react-i18next';
import type { DownloadErrorCode, DownloadJobDto, DownloadStatus, Language } from '../ipc/contracts';
import { defaultIpcClient } from '../ipc/client';
import { EmptyQueue } from './EmptyQueue';
import { formatTransferRate } from '../utils/formatTransferRate';

interface DownloadQueueProps {
  jobs: DownloadJobDto[];
  onDismissJob?: (id: string) => Promise<void>;
  onCancelJob?: (id: string) => Promise<void>;
  hideEmptyQueue?: boolean;
}

export const DownloadQueue: React.FC<DownloadQueueProps> = ({
  jobs,
  onDismissJob,
  onCancelJob,
  hideEmptyQueue = false,
}) => {
  const { t } = useTranslation();

  if (jobs.length === 0) {
    return hideEmptyQueue ? null : <EmptyQueue />;
  }

  return (
    <Stack spacing={2} sx={{ mt: 3 }}>
      <Typography variant="h6" sx={{ fontWeight: 700 }}>
        {t('queue.title')} ({jobs.length})
      </Typography>

      {jobs.map((job) => (
        <DownloadJobCard
          key={job.id}
          job={job}
          onDismiss={onDismissJob ? () => onDismissJob(job.id) : undefined}
          onCancel={onCancelJob ? () => onCancelJob(job.id) : undefined}
        />
      ))}
    </Stack>
  );
};

const DownloadJobCard: React.FC<{
  job: DownloadJobDto;
  onDismiss?: () => void;
  onCancel?: () => void;
}> = ({ job, onDismiss, onCancel }) => {
  const { t, i18n } = useTranslation();
  const currentLang = (i18n.language === 'en' ? 'en' : 'fr') as Language;

  const isVideo =
    job.preset.format === 'mp4' || job.preset.format === 'mov';

  const formatLabel = job.preset.format.toUpperCase();
  const rawQuality = job.preset.videoQuality ?? job.preset.mp3Quality;
  const qualityLabel = rawQuality
    ? rawQuality === 'best'
      ? t('dialog.bestQuality')
      : rawQuality.startsWith('p')
        ? `${rawQuality.slice(1)}p`
        : `${rawQuality.slice(1)} kb/s`
    : '';

  const isCompleted = job.status === 'completed';
  const isFailed = job.status === 'failed';
  const isDownloading = job.status === 'downloading';
  const isCancellable =
    job.status === 'queued' ||
    job.status === 'preparing' ||
    job.status === 'probing' ||
    job.status === 'downloading' ||
    job.status === 'converting' ||
    job.status === 'finalizing';
  const isTerminal =
    job.status === 'completed' ||
    job.status === 'failed' ||
    job.status === 'canceled';
  const isIndeterminateProgress =
    job.status === 'preparing' ||
    job.status === 'probing' ||
    job.status === 'converting' ||
    job.status === 'finalizing';

  const displaySpeed = isDownloading
    ? formatTransferRate(job.speedBytesPerSecond, currentLang)
    : null;

  const errorCode = job.errorDetails?.code as DownloadErrorCode | undefined;

  const errorMessage = errorCode && t(`errors.${errorCode}`) !== `errors.${errorCode}`
    ? t(`errors.${errorCode}`)
    : job.errorDetails?.message || job.errorMessage || t('errors.UNKNOWN_ERROR');

  const handleReveal = async () => {
    try {
      await defaultIpcClient.revealDownloadedFile(job.id);
    } catch (err) {
      console.error('Failed to reveal file:', err);
    }
  };

  const handleOpen = async () => {
    try {
      await defaultIpcClient.openDownloadedFile(job.id);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  };

  const handleOpenSourceUrl = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await defaultIpcClient.openDownloadSourceUrl(job.id);
    } catch (err) {
      console.error('Failed to open download source URL:', err);
    }
  };

  return (
    <Card
      variant="outlined"
      sx={{
        p: 2.5,
        borderRadius: 3,
        bgcolor: (theme) =>
          theme.palette.mode === 'dark' ? '#131b2e' : '#ffffff',
        borderColor: (theme) =>
          isFailed
            ? theme.palette.error.main
            : isCompleted
              ? theme.palette.success.main
              : 'divider',
        transition: 'all 0.2s ease-in-out',
        position: 'relative',
      }}
    >
      <CardContent sx={{ p: '0 !important' }}>
        <Stack spacing={1.5}>
          {/* Top Row: Icon + Title/URL + Format Chip + Status / Actions */}
          <Box
            sx={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              gap: 2,
            }}
          >
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, flex: 1, minWidth: 0 }}>
              {isVideo ? (
                <MovieIcon color="primary" />
              ) : (
                <MusicNoteIcon color="secondary" />
              )}
              <Box sx={{ minWidth: 0, flex: 1 }}>
                <Typography
                  variant="subtitle1"
                  sx={{
                    fontWeight: 600,
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                  }}
                >
                  {job.title || job.url}
                </Typography>
                <Typography
                  component="button"
                  type="button"
                  onClick={handleOpenSourceUrl}
                  sx={{
                    all: 'unset',
                    cursor: 'pointer',
                    color: 'text.secondary',
                    fontSize: '0.75rem',
                    lineHeight: 1.4,
                    display: 'block',
                    maxWidth: '100%',
                    whiteSpace: 'nowrap',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    textAlign: 'left',
                    '&:hover': {
                      textDecoration: 'underline',
                    },
                    '&:focus-visible': {
                      outline: '2px solid',
                      outlineColor: 'primary.main',
                      borderRadius: '2px',
                    },
                  }}
                >
                  {job.url}
                </Typography>
              </Box>
            </Box>

            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, flexShrink: 0 }}>
              <Chip
                size="small"
                label={`${formatLabel}${qualityLabel ? ` • ${qualityLabel}` : ''}`}
                variant="outlined"
                color="primary"
                sx={{ fontWeight: 600 }}
              />

              <StatusChip status={job.status} />

              {/* Completed Action Buttons */}
              {isCompleted && (
                <Stack direction="row" spacing={0.5} sx={{ ml: 0.5 }}>
                  <Tooltip title={t('queue.actions.showInFolder')} arrow>
                    <IconButton
                      size="small"
                      onClick={handleReveal}
                      aria-label={t('queue.actions.showInFolderAria')}
                      sx={{
                        border: '1px solid',
                        borderColor: (theme) =>
                          theme.palette.mode === 'dark'
                            ? 'rgba(255, 255, 255, 0.15)'
                            : 'rgba(0, 0, 0, 0.12)',
                        bgcolor: (theme) =>
                          theme.palette.mode === 'dark'
                            ? 'rgba(255, 255, 255, 0.05)'
                            : 'rgba(0, 0, 0, 0.03)',
                        '&:hover': {
                          bgcolor: 'action.hover',
                        },
                      }}
                    >
                      <FolderOpenIcon fontSize="small" />
                    </IconButton>
                  </Tooltip>

                  <Tooltip title={t('queue.actions.openFile')} arrow>
                    <IconButton
                      size="small"
                      onClick={handleOpen}
                      aria-label={t('queue.actions.openFileAria')}
                      color="primary"
                      sx={{
                        border: '1px solid',
                        borderColor: 'primary.main',
                        bgcolor: (theme) =>
                          theme.palette.mode === 'dark'
                            ? 'rgba(59, 130, 246, 0.12)'
                            : 'rgba(37, 99, 235, 0.08)',
                        '&:hover': {
                          bgcolor: 'primary.main',
                          color: '#ffffff',
                        },
                      }}
                    >
                      <PlayArrowIcon fontSize="small" />
                    </IconButton>
                  </Tooltip>
                </Stack>
              )}

              {/* Cancel Action Button (for active / queued jobs) */}
              {isCancellable && onCancel && (
                <Tooltip title={t('queue.actions.cancelAria')} arrow>
                  <IconButton
                    size="small"
                    onClick={onCancel}
                    aria-label={t('queue.actions.cancelAria')}
                    sx={{
                      color: 'text.secondary',
                      '&:hover': {
                        color: 'error.main',
                        bgcolor: 'action.hover',
                      },
                    }}
                  >
                    <CloseIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              )}

              {/* Dismiss Action Button (for terminal jobs) */}
              {isTerminal && onDismiss && (
                <Tooltip title={t('queue.actions.dismissAria')} arrow>
                  <IconButton
                    size="small"
                    onClick={onDismiss}
                    aria-label={t('queue.actions.dismissAria')}
                    sx={{
                      color: 'text.secondary',
                      '&:hover': {
                        color: 'error.main',
                        bgcolor: 'action.hover',
                      },
                    }}
                  >
                    <CloseIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              )}
            </Box>
          </Box>

          {/* Progress Bar (Determinate for Downloading, Indeterminate for Preparing/Converting) */}
          {(isDownloading || isIndeterminateProgress) && (
            <Box sx={{ width: '100%', mt: 0.5 }}>
              <LinearProgress
                variant={isDownloading ? 'determinate' : 'indeterminate'}
                value={isDownloading ? (job.progressPercent ?? 0) : undefined}
                sx={{
                  height: 6,
                  borderRadius: 3,
                  bgcolor: (theme) =>
                    theme.palette.mode === 'dark'
                      ? 'rgba(255, 255, 255, 0.08)'
                      : 'rgba(0, 0, 0, 0.06)',
                  '& .MuiLinearProgress-bar': {
                    borderRadius: 3,
                  },
                }}
              />
            </Box>
          )}

          {/* Downloading Stats: Percentage on left, Speed on right */}
          {isDownloading && (
            <Box
              sx={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                fontSize: '0.85rem',
                color: 'text.secondary',
                mt: 0.5,
              }}
            >
              <Typography variant="body2" sx={{ fontWeight: 600, color: 'text.primary' }}>
                {job.progressPercent ?? 0}{currentLang === 'fr' ? ' %' : '%'}
              </Typography>

              {displaySpeed && (
                <Typography variant="body2" sx={{ fontWeight: 500 }}>
                  {displaySpeed}
                </Typography>
              )}
            </Box>
          )}

          {/* Error Alert if Failed */}
          {isFailed && (
            <Alert
              severity="error"
              role="alert"
              sx={{ mt: 1, alignItems: 'center' }}
              action={
                errorCode ? (
                  <Tooltip title={`Code: ${errorCode}`} arrow>
                    <Chip
                      size="small"
                      label={errorCode}
                      variant="outlined"
                      color="error"
                      icon={<InfoOutlinedIcon fontSize="small" />}
                      sx={{ fontSize: '0.72rem', height: 22 }}
                    />
                  </Tooltip>
                ) : undefined
              }
            >
              <Typography variant="body2" sx={{ fontWeight: 500 }}>
                {errorMessage}
              </Typography>
            </Alert>
          )}
        </Stack>
      </CardContent>
    </Card>
  );
};

const StatusChip: React.FC<{ status: DownloadStatus }> = ({ status }) => {
  const { t } = useTranslation();

  switch (status) {
    case 'queued':
      return <Chip size="small" icon={<HourglassEmptyIcon fontSize="small" />} label={t('queue.status.queued')} />;
    case 'preparing':
      return <Chip size="small" color="info" label={t('queue.status.preparing')} />;
    case 'probing':
      return <Chip size="small" color="info" label={t('queue.status.probing')} />;
    case 'downloading':
      return <Chip size="small" color="primary" label={t('queue.status.downloading')} />;
    case 'converting':
      return <Chip size="small" color="secondary" label={t('queue.status.converting')} />;
    case 'finalizing':
      return <Chip size="small" color="info" label={t('queue.status.finalizing')} />;
    case 'completed':
      return (
        <Chip
          size="small"
          color="success"
          icon={<CheckCircleIcon fontSize="small" />}
          label={t('queue.status.completed')}
        />
      );
    case 'failed':
      return (
        <Chip
          size="small"
          color="error"
          icon={<ErrorIcon fontSize="small" />}
          label={t('queue.status.failed')}
        />
      );
    case 'canceled':
      return <Chip size="small" label={t('queue.status.canceled')} />;
    default:
      return <Chip size="small" label={status} />;
  }
};

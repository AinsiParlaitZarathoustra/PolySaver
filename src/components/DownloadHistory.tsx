// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React from 'react';
import {
  Box,
  Card,
  CardContent,
  Typography,
  Chip,
  Stack,
  Tooltip,
  IconButton,
} from '@mui/material';
import MovieIcon from '@mui/icons-material/Movie';
import MusicNoteIcon from '@mui/icons-material/MusicNote';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import CloseIcon from '@mui/icons-material/Close';
import { useTranslation } from 'react-i18next';
import type { DownloadHistoryEntryDto } from '../ipc/contracts';
import { defaultIpcClient } from '../ipc/client';

interface DownloadHistoryProps {
  entries: DownloadHistoryEntryDto[];
  onRemoveEntry: (id: string) => Promise<void>;
}

export const DownloadHistory: React.FC<DownloadHistoryProps> = ({
  entries,
  onRemoveEntry,
}) => {
  const { t } = useTranslation();

  if (entries.length === 0) {
    return null;
  }

  return (
    <Stack spacing={2} sx={{ mt: 4 }}>
      <Typography variant="h6" sx={{ fontWeight: 700 }}>
        {t('history.title')} ({entries.length})
      </Typography>

      {entries.map((entry) => (
        <DownloadHistoryCard
          key={entry.id}
          entry={entry}
          onRemove={() => onRemoveEntry(entry.id)}
        />
      ))}
    </Stack>
  );
};

const DownloadHistoryCard: React.FC<{
  entry: DownloadHistoryEntryDto;
  onRemove: () => void;
}> = ({ entry, onRemove }) => {
  const { t } = useTranslation();

  const isVideo =
    entry.preset.format === 'mp4' || entry.preset.format === 'mov';

  const formatLabel = entry.preset.format.toUpperCase();
  const rawQuality = entry.preset.videoQuality ?? entry.preset.mp3Quality;
  const qualityLabel = rawQuality
    ? rawQuality === 'best'
      ? t('dialog.bestQuality')
      : rawQuality.startsWith('p')
        ? `${rawQuality.slice(1)}p`
        : `${rawQuality.slice(1)} kb/s`
    : '';

  const handleReveal = async () => {
    try {
      await defaultIpcClient.revealHistoryFile(entry.id);
    } catch (err) {
      console.error('Failed to reveal history file:', err);
    }
  };

  const handleOpen = async () => {
    try {
      await defaultIpcClient.openHistoryFile(entry.id);
    } catch (err) {
      console.error('Failed to open history file:', err);
    }
  };

  const handleOpenSourceUrl = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await defaultIpcClient.openHistorySourceUrl(entry.id);
    } catch (err) {
      console.error('Failed to open history source URL:', err);
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
          theme.palette.mode === 'dark'
            ? 'rgba(255, 255, 255, 0.1)'
            : 'divider',
        transition: 'all 0.2s ease-in-out',
        position: 'relative',
      }}
    >
      <CardContent sx={{ p: '0 !important' }}>
        <Stack spacing={1.5}>
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
                  {entry.title || entry.sourceUrl}
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
                  {entry.sourceUrl}
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

                <Tooltip title={t('history.removeFromHistory')} arrow>
                  <IconButton
                    size="small"
                    onClick={onRemove}
                    aria-label={t('history.removeFromHistoryAria')}
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
              </Stack>
            </Box>
          </Box>
        </Stack>
      </CardContent>
    </Card>
  );
};

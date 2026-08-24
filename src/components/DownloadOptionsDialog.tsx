// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useEffect, useMemo } from 'react';
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  IconButton,
  Typography,
  Box,
  Button,
  ToggleButtonGroup,
  ToggleButton,
  Radio,
  RadioGroup,
  FormControlLabel,
  Skeleton,
  Stack,
  Divider,
  Paper,
  Tooltip,
  useMediaQuery,
  useTheme,
  Alert,
} from '@mui/material';
import CloseIcon from '@mui/icons-material/Close';
import DownloadIcon from '@mui/icons-material/Download';
import VideocamIcon from '@mui/icons-material/Videocam';
import AudiotrackIcon from '@mui/icons-material/Audiotrack';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import PlayCircleOutlineIcon from '@mui/icons-material/PlayCircleOutline';
import { useTranslation } from 'react-i18next';
import { defaultIpcClient } from '../ipc/client';
import type {
  DownloadPresetDto,
  Mp3Quality,
  OutputFormat,
  ProbeResult,
  VideoQuality,
} from '../ipc/contracts';

interface DownloadOptionsDialogProps {
  open: boolean;
  onClose: () => void;
  probeResult: ProbeResult | null;
  isLoading: boolean;
  errorMessage?: string | null;
  defaultPreset: DownloadPresetDto;
  defaultDownloadDirectory?: string;
  onConfirmDownload: (
    preset: DownloadPresetDto,
    outputDirectory?: string,
  ) => Promise<void>;
  isSubmitting?: boolean;
}

const ALLOWED_THUMBNAIL_HOSTS = [
  'i.ytimg.com',
  'img.youtube.com',
  'i1.ytimg.com',
  'i2.ytimg.com',
  'i3.ytimg.com',
  'i4.ytimg.com',
];

function isSafeThumbnailUrl(urlStr?: string | null): boolean {
  if (!urlStr) return false;
  try {
    const parsed = new URL(urlStr);
    if (parsed.protocol !== 'https:') return false;
    return (
      ALLOWED_THUMBNAIL_HOSTS.includes(parsed.hostname) ||
      parsed.hostname.endsWith('.ytimg.com')
    );
  } catch {
    return false;
  }
}

function formatDuration(seconds?: number | null): string {
  if (seconds === undefined || seconds === null || seconds < 0) return '';
  const hrs = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  const secs = Math.floor(seconds % 60);

  if (hrs > 0) {
    return `${hrs}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  }
  return `${mins}:${secs.toString().padStart(2, '0')}`;
}

function formatBytes(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return '';
  const mib = bytes / (1024 * 1024);
  if (mib >= 1024) {
    return `~${(mib / 1024).toFixed(1)} GB`;
  }
  return `~${Math.round(mib)} MB`;
}

function getQualityMeta(
  quality: VideoQuality,
  bestLabel: string,
): { quality: VideoQuality; label: string; height: number } {
  switch (quality) {
    case 'best':
      return { quality: 'best', label: bestLabel, height: 9999 };
    case 'p2160':
      return { quality: 'p2160', label: '2160p · 4K', height: 2160 };
    case 'p1440':
      return { quality: 'p1440', label: '1440p · 2K', height: 1440 };
    case 'p1080':
      return { quality: 'p1080', label: '1080p · Full HD', height: 1080 };
    case 'p720':
      return { quality: 'p720', label: '720p · HD', height: 720 };
    case 'p480':
      return { quality: 'p480', label: '480p · SD', height: 480 };
    case 'p360':
      return { quality: 'p360', label: '360p', height: 360 };
    case 'p240':
      return { quality: 'p240', label: '240p', height: 240 };
    case 'p144':
      return { quality: 'p144', label: '144p', height: 144 };
  }
}

export const DownloadOptionsDialog: React.FC<DownloadOptionsDialogProps> = ({
  open,
  onClose,
  probeResult,
  isLoading,
  errorMessage,
  defaultPreset,
  defaultDownloadDirectory = '',
  onConfirmDownload,
  isSubmitting = false,
}) => {
  const { t } = useTranslation();
  const theme = useTheme();
  const fullScreen = useMediaQuery(theme.breakpoints.down('sm'));

  // Local preset & directory state
  const [category, setCategory] = useState<'video' | 'audio'>('video');
  const [videoFormat, setVideoFormat] = useState<OutputFormat>('mp4');
  const [videoQuality, setVideoQuality] = useState<VideoQuality>('best');
  const [audioFormat, setAudioFormat] = useState<OutputFormat>('mp3');
  const [mp3Quality, setMp3Quality] = useState<Mp3Quality>('k320');
  const [selectedDirectory, setSelectedDirectory] = useState<string>(defaultDownloadDirectory);
  const [imgError, setImgError] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  // Derive dynamic video qualities based on probe result
  const videoQualities = useMemo(() => {
    const list: { quality: VideoQuality; label: string; height: number }[] = [
      getQualityMeta('best', t('dialog.bestQuality')),
    ];

    if (probeResult?.availableVideoQualities && probeResult.availableVideoQualities.length > 0) {
      for (const q of probeResult.availableVideoQualities) {
        if (q !== 'best') {
          list.push(getQualityMeta(q, t('dialog.bestQuality')));
        }
      }
    } else if (probeResult?.formats) {
      const heights = new Set<number>();
      for (const f of probeResult.formats) {
        if (f.hasVideo && f.height && f.height > 0) {
          heights.add(f.height);
        }
      }
      const sortedHeights = Array.from(heights).sort((a, b) => b - a);
      for (const h of sortedHeights) {
        const qKey = `p${h}` as VideoQuality;
        if (
          ['p144', 'p240', 'p360', 'p480', 'p720', 'p1080', 'p1440', 'p2160'].includes(qKey)
        ) {
          list.push(getQualityMeta(qKey, t('dialog.bestQuality')));
        }
      }
    }

    return list;
  }, [probeResult, t]);

  // Initialize or reset when dialog opens or probeResult arrives
  useEffect(() => {
    if (open) {
      setImgError(false);
      setSubmitError(null);
      setSelectedDirectory(defaultDownloadDirectory);
      if (defaultPreset.format === 'mp3' || defaultPreset.format === 'flac') {
        setCategory('audio');
        setAudioFormat(defaultPreset.format);
        setMp3Quality(defaultPreset.mp3Quality ?? 'k320');
      } else {
        setCategory('video');
        setVideoFormat(defaultPreset.format);

        // If default quality is available on this video, pick it; otherwise select 'best'
        const candidateQuality = defaultPreset.videoQuality ?? 'best';
        const isCandidateAvailable = videoQualities.some(
          (vq) => vq.quality === candidateQuality,
        );
        setVideoQuality(isCandidateAvailable ? candidateQuality : 'best');
      }
    }
  }, [open, defaultPreset, defaultDownloadDirectory, videoQualities]);

  const handleCategoryChange = (
    _event: React.MouseEvent<HTMLElement>,
    newCategory: 'video' | 'audio' | null,
  ) => {
    if (newCategory !== null) {
      setCategory(newCategory);
    }
  };

  const handleVideoFormatChange = (
    _event: React.MouseEvent<HTMLElement>,
    newFormat: OutputFormat | null,
  ) => {
    if (newFormat !== null) {
      setVideoFormat(newFormat);
    }
  };

  const handleAudioFormatChange = (
    _event: React.MouseEvent<HTMLElement>,
    newFormat: OutputFormat | null,
  ) => {
    if (newFormat !== null) {
      setAudioFormat(newFormat);
    }
  };

  const handlePickDirectory = async () => {
    try {
      const picked = await defaultIpcClient.pickDirectory(selectedDirectory);
      if (picked) {
        setSelectedDirectory(picked);
      }
    } catch {
      // Ignored: keep previous selection
    }
  };

  const handleConfirm = async () => {
    setSubmitError(null);
    let chosenPreset: DownloadPresetDto;

    if (category === 'video') {
      chosenPreset = {
        format: videoFormat,
        videoQuality,
      };
    } else {
      chosenPreset = {
        format: audioFormat,
        mp3Quality: audioFormat === 'mp3' ? mp3Quality : undefined,
      };
    }

    try {
      await onConfirmDownload(chosenPreset, selectedDirectory);
    } catch (err) {
      setSubmitError(
        err instanceof Error ? err.message : t('errors.DOWNLOAD_PROCESS_FAILED'),
      );
    }
  };

  const getApproxSizeForQuality = (quality: VideoQuality): string | null => {
    if (!probeResult?.formats) return null;
    const targetHeight =
      videoQualities.find((q) => q.quality === quality)?.height ?? 0;

    if (quality === 'best') {
      const match = probeResult.formats.find((f) => f.filesizeApproxBytes);
      return match?.filesizeApproxBytes ? formatBytes(match.filesizeApproxBytes) : null;
    }

    const match = probeResult.formats.find(
      (f) => f.height === targetHeight && f.filesizeApproxBytes,
    );
    return match?.filesizeApproxBytes ? formatBytes(match.filesizeApproxBytes) : null;
  };

  const hasSafeThumbnail =
    !imgError && isSafeThumbnailUrl(probeResult?.thumbnailUrl);

  return (
    <Dialog
      open={open}
      onClose={isSubmitting ? undefined : onClose}
      fullWidth
      maxWidth="md"
      fullScreen={fullScreen}
      aria-labelledby="download-options-dialog-title"
    >
      <DialogTitle
        id="download-options-dialog-title"
        sx={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          pb: 1.5,
        }}
      >
        <Typography variant="h6" component="span" sx={{ fontWeight: 700 }}>
          {t('dialog.title')}
        </Typography>
        <IconButton
          aria-label={t('common.close')}
          onClick={onClose}
          disabled={isSubmitting}
          size="small"
        >
          <CloseIcon />
        </IconButton>
      </DialogTitle>

      <DialogContent dividers sx={{ p: { xs: 2, sm: 3 } }}>
        <Stack spacing={3}>
          {/* Top Section: Thumbnail & Metadata */}
          {isLoading ? (
            <Box
              sx={{
                display: 'flex',
                flexDirection: { xs: 'column', sm: 'row' },
                gap: 2.5,
                alignItems: { xs: 'stretch', sm: 'center' },
              }}
            >
              <Skeleton
                variant="rectangular"
                sx={{
                  width: { xs: '100%', sm: 280 },
                  height: { xs: 180, sm: 158 },
                  borderRadius: 2,
                }}
              />
              <Box sx={{ flex: 1 }}>
                <Skeleton variant="text" height={32} width="90%" />
                <Skeleton variant="text" height={24} width="60%" />
                <Skeleton variant="text" height={20} width="40%" />
              </Box>
            </Box>
          ) : errorMessage ? (
            <Alert severity="error">{errorMessage}</Alert>
          ) : probeResult ? (
            <Box
              sx={{
                display: 'flex',
                flexDirection: { xs: 'column', sm: 'row' },
                gap: 2.5,
                alignItems: { xs: 'stretch', sm: 'flex-start' },
              }}
            >
              {/* Large 16:9 Thumbnail or Safe Fallback */}
              <Box
                sx={{
                  width: { xs: '100%', sm: 300 },
                  height: { xs: 170, sm: 169 },
                  flexShrink: 0,
                  borderRadius: '12px',
                  overflow: 'hidden',
                  bgcolor: 'action.hover',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  position: 'relative',
                  border: 1,
                  borderColor: 'divider',
                }}
              >
                {hasSafeThumbnail && probeResult.thumbnailUrl ? (
                  <Box
                    component="img"
                    src={probeResult.thumbnailUrl}
                    alt={probeResult.title}
                    onError={() => setImgError(true)}
                    sx={{
                      width: '100%',
                      height: '100%',
                      objectFit: 'cover',
                    }}
                  />
                ) : (
                  <PlayCircleOutlineIcon
                    sx={{ fontSize: 56, color: 'text.secondary', opacity: 0.6 }}
                  />
                )}
                {probeResult.durationSeconds !== undefined && (
                  <Box
                    sx={{
                      position: 'absolute',
                      bottom: 8,
                      right: 8,
                      bgcolor: 'rgba(0, 0, 0, 0.8)',
                      color: '#ffffff',
                      px: 1,
                      py: 0.25,
                      borderRadius: '4px',
                      fontSize: '0.75rem',
                      fontWeight: 600,
                    }}
                  >
                    {formatDuration(probeResult.durationSeconds)}
                  </Box>
                )}
              </Box>

              {/* Title & Channel Info */}
              <Box sx={{ flex: 1, minWidth: 0 }}>
                <Typography
                  variant="h6"
                  sx={{
                    fontWeight: 700,
                    fontSize: { xs: '1.05rem', sm: '1.15rem' },
                    lineHeight: 1.3,
                    mb: 1,
                    display: '-webkit-box',
                    WebkitLineClamp: 3,
                    WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}
                >
                  {probeResult.title}
                </Typography>
                {probeResult.uploader && (
                  <Typography
                    variant="body2"
                    color="text.secondary"
                    sx={{ fontWeight: 500, mb: 0.5 }}
                  >
                    {probeResult.uploader}
                  </Typography>
                )}
              </Box>
            </Box>
          ) : null}

          <Divider />

          {/* Type Selector: Video vs Audio */}
          <Box>
            <Typography variant="subtitle2" sx={{ fontWeight: 700, mb: 1 }}>
              {t('dialog.typeSelector')}
            </Typography>
            <ToggleButtonGroup
              value={category}
              exclusive
              onChange={handleCategoryChange}
              fullWidth
              size="medium"
            >
              <ToggleButton value="video" sx={{ gap: 1, py: 1 }}>
                <VideocamIcon fontSize="small" />
                <Typography variant="body2" sx={{ fontWeight: 600 }}>
                  {t('common.video')}
                </Typography>
              </ToggleButton>
              <ToggleButton value="audio" sx={{ gap: 1, py: 1 }}>
                <AudiotrackIcon fontSize="small" />
                <Typography variant="body2" sx={{ fontWeight: 600 }}>
                  {t('common.audio')}
                </Typography>
              </ToggleButton>
            </ToggleButtonGroup>
          </Box>

          {/* Contextual Format Selector */}
          <Box>
            <Typography variant="subtitle2" sx={{ fontWeight: 700, mb: 1 }}>
              {t('dialog.formatSelector')}
            </Typography>
            {category === 'video' ? (
              <ToggleButtonGroup
                value={videoFormat}
                exclusive
                onChange={handleVideoFormatChange}
                fullWidth
                size="small"
              >
                <ToggleButton value="mp4" sx={{ fontWeight: 600, py: 0.75 }}>
                  MP4
                </ToggleButton>
                <ToggleButton value="mov" sx={{ fontWeight: 600, py: 0.75 }}>
                  MOV
                </ToggleButton>
              </ToggleButtonGroup>
            ) : (
              <ToggleButtonGroup
                value={audioFormat}
                exclusive
                onChange={handleAudioFormatChange}
                fullWidth
                size="small"
              >
                <ToggleButton value="mp3" sx={{ fontWeight: 600, py: 0.75 }}>
                  MP3
                </ToggleButton>
                <ToggleButton value="flac" sx={{ fontWeight: 600, py: 0.75 }}>
                  FLAC
                </ToggleButton>
              </ToggleButtonGroup>
            )}
          </Box>

          {/* Quality Options Section */}
          <Box>
            <Typography variant="subtitle2" sx={{ fontWeight: 700, mb: 1.5 }}>
              {t('dialog.qualitySelector')}
            </Typography>

            {category === 'video' ? (
              <RadioGroup
                value={videoQuality}
                onChange={(e) => setVideoQuality(e.target.value as VideoQuality)}
              >
                <Stack spacing={1}>
                  {videoQualities.map((vq) => {
                    const sizeStr = getApproxSizeForQuality(vq.quality);
                    const isSelected = videoQuality === vq.quality;
                    return (
                      <Paper
                        key={vq.quality}
                        variant="outlined"
                        onClick={() => setVideoQuality(vq.quality)}
                        sx={{
                          p: 1.25,
                          px: 2,
                          cursor: 'pointer',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'space-between',
                          borderRadius: '10px',
                          borderColor: isSelected ? 'primary.main' : 'divider',
                          bgcolor: isSelected ? 'action.selected' : 'transparent',
                          '&:hover': {
                            bgcolor: 'action.hover',
                          },
                        }}
                      >
                        <FormControlLabel
                          value={vq.quality}
                          control={<Radio size="small" />}
                          label={
                            <Typography
                              variant="body2"
                              sx={{ fontWeight: isSelected ? 700 : 500 }}
                            >
                              {vq.label}
                            </Typography>
                          }
                          sx={{ m: 0, flex: 1 }}
                        />
                        {sizeStr && (
                          <Typography
                            variant="caption"
                            color="text.secondary"
                            sx={{ fontWeight: 600 }}
                          >
                            {sizeStr}
                          </Typography>
                        )}
                      </Paper>
                    );
                  })}
                </Stack>
              </RadioGroup>
            ) : audioFormat === 'mp3' ? (
              <RadioGroup
                value={mp3Quality}
                onChange={(e) => setMp3Quality(e.target.value as Mp3Quality)}
              >
                <Stack spacing={1}>
                  {[
                    { quality: 'k320' as Mp3Quality, label: '320 kb/s' },
                    { quality: 'k256' as Mp3Quality, label: '256 kb/s' },
                    { quality: 'k192' as Mp3Quality, label: '192 kb/s' },
                    { quality: 'k128' as Mp3Quality, label: '128 kb/s' },
                  ].map((mq) => {
                    const isSelected = mp3Quality === mq.quality;
                    return (
                      <Paper
                        key={mq.quality}
                        variant="outlined"
                        onClick={() => setMp3Quality(mq.quality)}
                        sx={{
                          p: 1.25,
                          px: 2,
                          cursor: 'pointer',
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent: 'space-between',
                          borderRadius: '10px',
                          borderColor: isSelected ? 'primary.main' : 'divider',
                          bgcolor: isSelected ? 'action.selected' : 'transparent',
                          '&:hover': {
                            bgcolor: 'action.hover',
                          },
                        }}
                      >
                        <FormControlLabel
                          value={mq.quality}
                          control={<Radio size="small" />}
                          label={
                            <Typography
                              variant="body2"
                              sx={{ fontWeight: isSelected ? 700 : 500 }}
                            >
                              {mq.label}
                            </Typography>
                          }
                          sx={{ m: 0 }}
                        />
                      </Paper>
                    );
                  })}
                </Stack>
              </RadioGroup>
            ) : (
              <Paper
                variant="outlined"
                sx={{
                  p: 2,
                  borderRadius: '10px',
                  bgcolor: 'action.hover',
                  borderColor: 'divider',
                }}
              >
                <Typography variant="body2" sx={{ fontWeight: 600 }}>
                  FLAC · {t('dialog.losslessQuality')}
                </Typography>
              </Paper>
            )}
          </Box>

          <Divider />

          {/* Location Selector (Per-download output directory) */}
          <Box
            sx={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 2,
              p: 1.5,
              borderRadius: '10px',
              bgcolor: (tTheme) =>
                tTheme.palette.mode === 'dark'
                  ? 'rgba(255,255,255,0.03)'
                  : 'rgba(0,0,0,0.02)',
              border: 1,
              borderColor: 'divider',
            }}
          >
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5, minWidth: 0, flex: 1 }}>
              <Button
                variant="outlined"
                size="small"
                startIcon={<FolderOpenIcon />}
                onClick={handlePickDirectory}
                disabled={isSubmitting}
                aria-label={t('dialog.chooseLocationAria')}
                sx={{
                  borderRadius: '8px',
                  textTransform: 'none',
                  fontWeight: 600,
                  whiteSpace: 'nowrap',
                  flexShrink: 0,
                }}
              >
                {t('dialog.chooseLocation')}
              </Button>
              <Tooltip title={selectedDirectory}>
                <Typography
                  variant="body2"
                  color="text.secondary"
                  sx={{
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                    fontSize: '0.85rem',
                  }}
                >
                  {selectedDirectory}
                </Typography>
              </Tooltip>
            </Box>
          </Box>

          {submitError && (
            <Alert severity="error" onClose={() => setSubmitError(null)}>
              {submitError}
            </Alert>
          )}
        </Stack>
      </DialogContent>

      <DialogActions sx={{ px: { xs: 2, sm: 3 }, py: 2 }}>
        <Button
          variant="outlined"
          color="inherit"
          onClick={onClose}
          disabled={isSubmitting}
          sx={{ borderRadius: '10px', px: 2.5 }}
        >
          {t('common.cancel')}
        </Button>
        <Button
          variant="contained"
          color="primary"
          onClick={handleConfirm}
          disabled={isSubmitting || isLoading || !probeResult}
          startIcon={<DownloadIcon />}
          sx={{ borderRadius: '10px', px: 3, fontWeight: 700 }}
        >
          {t('dialog.startDownload')}
        </Button>
      </DialogActions>
    </Dialog>
  );
};

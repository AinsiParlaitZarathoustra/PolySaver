// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useEffect, useRef } from 'react';
import {
  Drawer,
  Box,
  Typography,
  IconButton,
  Divider,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  FormControlLabel,
  Switch,
  TextField,
  Card,
  Chip,
  Stack,
  ToggleButtonGroup,
  ToggleButton,
  Tooltip,
  Alert,
  CircularProgress,
  Button,
} from '@mui/material';
import CloseIcon from '@mui/icons-material/Close';
import LightModeIcon from '@mui/icons-material/LightMode';
import DarkModeIcon from '@mui/icons-material/DarkMode';
import BrightnessAutoIcon from '@mui/icons-material/BrightnessAuto';
import VideocamIcon from '@mui/icons-material/Videocam';
import AudiotrackIcon from '@mui/icons-material/Audiotrack';
import RefreshIcon from '@mui/icons-material/Refresh';
import CheckCircleOutlineIcon from '@mui/icons-material/CheckCircleOutline';
import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import CheckIcon from '@mui/icons-material/Check';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import { useTranslation } from 'react-i18next';
import type {
  AppSettingsDto,
  HealthStatus,
  Language,
  Mp3Quality,
  OutputFormat,
  ThemeMode,
  VideoQuality,
} from '../ipc/contracts';
import { defaultIpcClient } from '../ipc/client';
import type { AutosaveStatus } from '../features/settings/useAutosaveSettings';

interface SettingsDrawerProps {
  open: boolean;
  onClose: () => void;
  settings: AppSettingsDto;
  onUpdateSettings: (
    update: Partial<AppSettingsDto> | ((prev: AppSettingsDto) => AppSettingsDto),
    immediate?: boolean,
  ) => void;
  saveStatus: AutosaveStatus;
  errorMessage?: string | null;
  onBrowseDirectory?: (defaultPath?: string) => Promise<string | null>;
}

export const SettingsDrawer: React.FC<SettingsDrawerProps> = ({
  open,
  onClose,
  settings,
  onUpdateSettings,
  saveStatus,
  errorMessage,
  onBrowseDirectory,
}) => {
  const { t, i18n } = useTranslation();

  // Session memory for category toggling
  const lastVideoPresetRef = useRef<{
    format: OutputFormat;
    videoQuality: VideoQuality;
  }>({
    format:
      settings.defaultPreset.format === 'mov' ? 'mov' : 'mp4',
    videoQuality: settings.defaultPreset.videoQuality ?? 'p1080',
  });

  const lastAudioPresetRef = useRef<{
    format: OutputFormat;
    mp3Quality?: Mp3Quality;
  }>({
    format:
      settings.defaultPreset.format === 'flac' ? 'flac' : 'mp3',
    mp3Quality: settings.defaultPreset.mp3Quality ?? 'k320',
  });

  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [loadingHealth, setLoadingHealth] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);

  // Derive active category from current preset
  const isAudio =
    settings.defaultPreset.format === 'mp3' ||
    settings.defaultPreset.format === 'flac';
  const category = isAudio ? 'audio' : 'video';

  useEffect(() => {
    if (open) {
      setBrowseError(null);
      fetchHealth();
    }
  }, [open]);

  // Keep session memory updated
  useEffect(() => {
    if (
      settings.defaultPreset.format === 'mp4' ||
      settings.defaultPreset.format === 'mov'
    ) {
      lastVideoPresetRef.current = {
        format: settings.defaultPreset.format,
        videoQuality: settings.defaultPreset.videoQuality ?? 'p1080',
      };
    } else {
      lastAudioPresetRef.current = {
        format: settings.defaultPreset.format,
        mp3Quality: settings.defaultPreset.mp3Quality ?? 'k320',
      };
    }
  }, [settings.defaultPreset]);

  const fetchHealth = async () => {
    setLoadingHealth(true);
    try {
      const res = await defaultIpcClient.healthCheck();
      setHealth(res);
    } catch {
      // Ignore health check error in UI
    } finally {
      setLoadingHealth(false);
    }
  };

  const handleThemeChange = (
    _event: React.MouseEvent<HTMLElement>,
    newTheme: ThemeMode | null,
  ) => {
    if (newTheme !== null) {
      onUpdateSettings({ themeMode: newTheme }, true);
    }
  };

  const handleLanguageChange = (
    _event: React.MouseEvent<HTMLElement>,
    newLang: Language | null,
  ) => {
    if (newLang !== null) {
      onUpdateSettings({ language: newLang }, true);
    }
  };

  const handleParallelChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const checked = e.target.checked;
    onUpdateSettings(
      {
        parallelDownloads: checked,
      },
      true,
    );
  };

  const handleMaxConcurrentChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const parsed = parseInt(e.target.value, 10);
    if (!Number.isNaN(parsed) && parsed >= 1 && parsed <= 8) {
      onUpdateSettings(
        {
          maxConcurrent: parsed,
        },
        true,
      );
    }
  };

  const handleDirectoryChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setBrowseError(null);
    onUpdateSettings({ downloadDirectory: e.target.value }, false);
  };

  const handleBrowseClick = async () => {
    setBrowseError(null);
    try {
      const pickerFn = onBrowseDirectory ?? ((path) => defaultIpcClient.pickDirectory(path));
      const selected = await pickerFn(settings.downloadDirectory);
      if (selected && selected.trim().length > 0) {
        onUpdateSettings({ downloadDirectory: selected }, true);
      }
    } catch {
      setBrowseError(t('settings.directory.browseError'));
    }
  };

  const handleCategoryChange = (
    _event: React.MouseEvent<HTMLElement>,
    newCategory: 'video' | 'audio' | null,
  ) => {
    if (newCategory === null || newCategory === category) return;

    if (newCategory === 'video') {
      const last = lastVideoPresetRef.current;
      onUpdateSettings(
        {
          defaultPreset: {
            format: last.format,
            videoQuality: last.videoQuality,
          },
        },
        true,
      );
    } else {
      const last = lastAudioPresetRef.current;
      onUpdateSettings(
        {
          defaultPreset: {
            format: last.format,
            mp3Quality: last.format === 'mp3' ? (last.mp3Quality ?? 'k320') : undefined,
          },
        },
        true,
      );
    }
  };

  const handleFormatChange = (newFormat: OutputFormat) => {
    if (newFormat === 'mp4' || newFormat === 'mov') {
      onUpdateSettings(
        {
          defaultPreset: {
            format: newFormat,
            videoQuality: settings.defaultPreset.videoQuality ?? 'p1080',
          },
        },
        true,
      );
    } else if (newFormat === 'mp3') {
      onUpdateSettings(
        {
          defaultPreset: {
            format: 'mp3',
            mp3Quality: settings.defaultPreset.mp3Quality ?? 'k320',
          },
        },
        true,
      );
    } else {
      onUpdateSettings(
        {
          defaultPreset: {
            format: 'flac',
          },
        },
        true,
      );
    }
  };

  const handleVideoQualityChange = (quality: VideoQuality) => {
    onUpdateSettings(
      {
        defaultPreset: {
          format: settings.defaultPreset.format,
          videoQuality: quality,
        },
      },
      true,
    );
  };

  const handleMp3QualityChange = (quality: Mp3Quality) => {
    onUpdateSettings(
      {
        defaultPreset: {
          format: 'mp3',
          mp3Quality: quality,
        },
      },
      true,
    );
  };

  const activeLanguage = settings.language ?? (i18n.language === 'en' ? 'en' : 'fr');

  return (
    <Drawer
      anchor="right"
      open={open}
      onClose={onClose}
      PaperProps={{
        sx: {
          width: { xs: '100%', sm: 420 },
          p: 3,
          boxSizing: 'border-box',
          bgcolor: 'background.paper',
        },
      }}
    >
      {/* Header with Title & Live Save Status */}
      <Box
        sx={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          mb: 2,
        }}
      >
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1.5 }}>
          <Typography variant="h6" sx={{ fontWeight: 700 }}>
            {t('settings.title')}
          </Typography>
          {saveStatus === 'saving' && (
            <Chip
              icon={<CircularProgress size={12} color="inherit" />}
              label={t('settings.autosave.saving')}
              size="small"
              variant="outlined"
              sx={{ height: 22, fontSize: '0.75rem' }}
            />
          )}
          {saveStatus === 'saved' && (
            <Chip
              icon={<CheckIcon fontSize="small" />}
              label={t('settings.autosave.saved')}
              size="small"
              color="success"
              variant="outlined"
              sx={{ height: 22, fontSize: '0.75rem' }}
            />
          )}
        </Box>
        <IconButton onClick={onClose} aria-label={t('settings.closeAria')} size="small">
          <CloseIcon />
        </IconButton>
      </Box>

      <Divider sx={{ mb: 2.5 }} />

      <Stack spacing={3} sx={{ flexGrow: 1, overflowY: 'auto' }}>
        {/* Language Selector [ 🇫🇷 Français ] [ 🇬🇧 English ] */}
        <Box>
          <Typography variant="subtitle2" sx={{ mb: 1, fontWeight: 700 }}>
            {t('settings.language.label')}
          </Typography>
          <ToggleButtonGroup
            value={activeLanguage}
            exclusive
            onChange={handleLanguageChange}
            fullWidth
            size="small"
            aria-label={t('settings.language.label')}
          >
            <ToggleButton value="fr" aria-label={t('settings.language.frAria')}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <span role="img" aria-label="France">🇫🇷</span>
                <Typography variant="body2">{t('settings.language.fr')}</Typography>
              </Box>
            </ToggleButton>
            <ToggleButton value="en" aria-label={t('settings.language.enAria')}>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <span role="img" aria-label="United Kingdom">🇬🇧</span>
                <Typography variant="body2">{t('settings.language.en')}</Typography>
              </Box>
            </ToggleButton>
          </ToggleButtonGroup>
        </Box>

        {/* Theme Mode Toggle (3 Icons: Light, Dark, System) */}
        <Box>
          <Typography variant="subtitle2" sx={{ mb: 1, fontWeight: 700 }}>
            {t('settings.theme.label')}
          </Typography>
          <ToggleButtonGroup
            value={settings.themeMode}
            exclusive
            onChange={handleThemeChange}
            fullWidth
            size="small"
            aria-label={t('settings.theme.label')}
          >
            <ToggleButton value="light" aria-label={t('settings.theme.lightAria')}>
              <Tooltip title={t('settings.theme.light')}>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                  <LightModeIcon fontSize="small" />
                  <Typography variant="body2">{t('settings.theme.light')}</Typography>
                </Box>
              </Tooltip>
            </ToggleButton>
            <ToggleButton value="dark" aria-label={t('settings.theme.darkAria')}>
              <Tooltip title={t('settings.theme.dark')}>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                  <DarkModeIcon fontSize="small" />
                  <Typography variant="body2">{t('settings.theme.dark')}</Typography>
                </Box>
              </Tooltip>
            </ToggleButton>
            <ToggleButton value="system" aria-label={t('settings.theme.systemAria')}>
              <Tooltip title={t('settings.theme.system')}>
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                  <BrightnessAutoIcon fontSize="small" />
                  <Typography variant="body2">{t('settings.theme.system')}</Typography>
                </Box>
              </Tooltip>
            </ToggleButton>
          </ToggleButtonGroup>
        </Box>

        {/* Download Directory with Native Browse Button */}
        <Box>
          <Typography variant="subtitle2" sx={{ mb: 1, fontWeight: 700 }}>
            {t('settings.directory.label')}
          </Typography>
          <Box sx={{ display: 'flex', gap: 1, alignItems: 'center' }}>
            <TextField
              fullWidth
              size="small"
              value={settings.downloadDirectory}
              onChange={handleDirectoryChange}
              placeholder={t('settings.directory.placeholder')}
              inputProps={{ 'aria-label': t('settings.directory.label') }}
            />
            <Button
              variant="outlined"
              size="small"
              startIcon={<FolderOpenIcon />}
              onClick={handleBrowseClick}
              aria-label={t('settings.directory.browseAria')}
              sx={{
                whiteSpace: 'nowrap',
                minWidth: 'auto',
                height: 40,
                px: 1.5,
                fontWeight: 600,
                textTransform: 'none',
              }}
            >
              {t('settings.directory.browseButton')}
            </Button>
          </Box>
          {browseError && (
            <Alert severity="error" sx={{ mt: 1 }} onClose={() => setBrowseError(null)}>
              {browseError}
            </Alert>
          )}
        </Box>

        {/* Default Format: Exclusive Category & Contextual Selectors */}
        <Box>
          <Typography variant="subtitle2" sx={{ mb: 1, fontWeight: 700 }}>
            {t('settings.defaultPreset.label')}
          </Typography>
          <Stack spacing={1.5}>
            {/* Category Toggle [ Vidéo ] [ Audio ] */}
            <ToggleButtonGroup
              value={category}
              exclusive
              onChange={handleCategoryChange}
              fullWidth
              size="small"
            >
              <ToggleButton value="video" sx={{ gap: 0.75 }}>
                <VideocamIcon fontSize="small" />
                <Typography variant="body2" sx={{ fontWeight: 600 }}>
                  {t('common.video')}
                </Typography>
              </ToggleButton>
              <ToggleButton value="audio" sx={{ gap: 0.75 }}>
                <AudiotrackIcon fontSize="small" />
                <Typography variant="body2" sx={{ fontWeight: 600 }}>
                  {t('common.audio')}
                </Typography>
              </ToggleButton>
            </ToggleButtonGroup>

            {/* Video Format & Quality */}
            {category === 'video' && (
              <>
                <FormControl fullWidth size="small">
                  <InputLabel id="drawer-video-format-label">{t('common.format')}</InputLabel>
                  <Select
                    labelId="drawer-video-format-label"
                    value={settings.defaultPreset.format}
                    label={t('common.format')}
                    onChange={(e) =>
                      handleFormatChange(e.target.value as OutputFormat)
                    }
                  >
                    <MenuItem value="mp4">MP4</MenuItem>
                    <MenuItem value="mov">MOV</MenuItem>
                  </Select>
                </FormControl>

                <FormControl fullWidth size="small">
                  <InputLabel id="drawer-video-quality-label">{t('common.quality')}</InputLabel>
                  <Select
                    labelId="drawer-video-quality-label"
                    value={settings.defaultPreset.videoQuality ?? 'p1080'}
                    label={t('common.quality')}
                    onChange={(e) =>
                      handleVideoQualityChange(e.target.value as VideoQuality)
                    }
                  >
                    <MenuItem value="best">{t('dialog.bestQuality')}</MenuItem>
                    <MenuItem value="p2160">2160p · 4K</MenuItem>
                    <MenuItem value="p1440">1440p · 2K</MenuItem>
                    <MenuItem value="p1080">1080p · Full HD</MenuItem>
                    <MenuItem value="p720">720p · HD</MenuItem>
                    <MenuItem value="p480">480p · SD</MenuItem>
                    <MenuItem value="p360">360p</MenuItem>
                  </Select>
                </FormControl>
              </>
            )}

            {/* Audio Format & Quality */}
            {category === 'audio' && (
              <>
                <FormControl fullWidth size="small">
                  <InputLabel id="drawer-audio-format-label">{t('common.format')}</InputLabel>
                  <Select
                    labelId="drawer-audio-format-label"
                    value={settings.defaultPreset.format}
                    label={t('common.format')}
                    onChange={(e) =>
                      handleFormatChange(e.target.value as OutputFormat)
                    }
                  >
                    <MenuItem value="mp3">MP3</MenuItem>
                    <MenuItem value="flac">FLAC</MenuItem>
                  </Select>
                </FormControl>

                {settings.defaultPreset.format === 'mp3' && (
                  <FormControl fullWidth size="small">
                    <InputLabel id="drawer-mp3-quality-label">{t('common.quality')}</InputLabel>
                    <Select
                      labelId="drawer-mp3-quality-label"
                      value={settings.defaultPreset.mp3Quality ?? 'k320'}
                      label={t('common.quality')}
                      onChange={(e) =>
                        handleMp3QualityChange(e.target.value as Mp3Quality)
                      }
                    >
                      <MenuItem value="k320">320 kb/s</MenuItem>
                      <MenuItem value="k256">256 kb/s</MenuItem>
                      <MenuItem value="k192">192 kb/s</MenuItem>
                      <MenuItem value="k128">128 kb/s</MenuItem>
                    </Select>
                  </FormControl>
                )}

                {settings.defaultPreset.format === 'flac' && (
                  <Typography
                    variant="caption"
                    color="text.secondary"
                    sx={{ px: 0.5 }}
                  >
                    FLAC · {t('dialog.losslessQuality')}
                  </Typography>
                )}
              </>
            )}
          </Stack>
        </Box>

        {/* Parallel Downloads Switch */}
        <Box>
          <FormControlLabel
            control={
              <Switch
                checked={settings.parallelDownloads}
                onChange={handleParallelChange}
                color="primary"
              />
            }
            label={
              <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                {t('settings.concurrency.label')}
              </Typography>
            }
          />
          {settings.parallelDownloads && (
            <Box sx={{ mt: 1.5, pl: 2 }}>
              <TextField
                select
                size="small"
                label={i18n.language === 'fr' ? 'Téléchargements simultanés' : 'Max concurrent downloads'}
                value={settings.maxConcurrent || 3}
                onChange={handleMaxConcurrentChange}
                sx={{ width: 240 }}
              >
                {[1, 2, 3, 4, 5, 6, 7, 8].map((num) => (
                  <MenuItem key={num} value={num}>
                    {num}
                  </MenuItem>
                ))}
              </TextField>
            </Box>
          )}
        </Box>

        {/* Compact Diagnostics */}
        <Box>
          <Box
            sx={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              mb: 1,
            }}
          >
            <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
              {t('settings.diagnostics.label')}
            </Typography>
            <IconButton
              size="small"
              onClick={fetchHealth}
              disabled={loadingHealth}
              aria-label={t('settings.diagnostics.label')}
            >
              {loadingHealth ? (
                <CircularProgress size={14} />
              ) : (
                <RefreshIcon fontSize="small" />
              )}
            </IconButton>
          </Box>

          <Stack spacing={1}>
            <Card variant="outlined" sx={{ p: 1.25 }}>
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                }}
              >
                <Box>
                  <Typography variant="body2" sx={{ fontWeight: 600 }}>
                    yt-dlp
                  </Typography>
                  {health?.ytdlp.version && (
                    <Typography variant="caption" color="text.secondary">
                      v{health.ytdlp.version}
                    </Typography>
                  )}
                </Box>
                <Chip
                  size="small"
                  icon={
                    health?.ytdlp.isReady ? (
                      <CheckCircleOutlineIcon fontSize="small" />
                    ) : (
                      <ErrorOutlineIcon fontSize="small" />
                    )
                  }
                  label={health?.ytdlp.isReady ? t('settings.diagnostics.ready') : t('settings.diagnostics.unavailable')}
                  color={health?.ytdlp.isReady ? 'success' : 'error'}
                  variant="outlined"
                  sx={{ height: 22, fontSize: '0.75rem' }}
                />
              </Box>
            </Card>

            <Card variant="outlined" sx={{ p: 1.25 }}>
              <Box
                sx={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                }}
              >
                <Box>
                  <Typography variant="body2" sx={{ fontWeight: 600 }}>
                    FFmpeg
                  </Typography>
                  {health?.ffmpeg.version && (
                    <Typography variant="caption" color="text.secondary">
                      {health.ffmpeg.version.split(' ').slice(0, 3).join(' ')}
                    </Typography>
                  )}
                </Box>
                <Chip
                  size="small"
                  icon={
                    health?.ffmpeg.isReady ? (
                      <CheckCircleOutlineIcon fontSize="small" />
                    ) : (
                      <ErrorOutlineIcon fontSize="small" />
                    )
                  }
                  label={health?.ffmpeg.isReady ? t('settings.diagnostics.ready') : t('settings.diagnostics.unavailable')}
                  color={health?.ffmpeg.isReady ? 'success' : 'error'}
                  variant="outlined"
                  sx={{ height: 22, fontSize: '0.75rem' }}
                />
              </Box>
            </Card>
          </Stack>
        </Box>

        {errorMessage && <Alert severity="error">{errorMessage}</Alert>}
      </Stack>
    </Drawer>
  );
};

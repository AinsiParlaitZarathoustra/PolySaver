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
import LaptopIcon from '@mui/icons-material/Laptop';
import VideocamIcon from '@mui/icons-material/Videocam';
import AudiotrackIcon from '@mui/icons-material/Audiotrack';
import RefreshIcon from '@mui/icons-material/Refresh';
import UpgradeIcon from '@mui/icons-material/Upgrade';
import CheckCircleOutlineIcon from '@mui/icons-material/CheckCircleOutline';
import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import CheckIcon from '@mui/icons-material/Check';
import FolderOpenIcon from '@mui/icons-material/FolderOpen';
import { useTranslation } from 'react-i18next';
import type {
  AppSettingsDto,
  CookiesBrowser,
  EngineChannel,
  EngineUpdateStatusDto,
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
  /** Allows reopening the JavaScript runtime setup dialog from diagnostics. */
  onOpenJsRuntimeSetup?: () => void;
}

export const SettingsDrawer: React.FC<SettingsDrawerProps> = ({
  open,
  onClose,
  settings,
  onUpdateSettings,
  saveStatus,
  errorMessage,
  onBrowseDirectory,
  onOpenJsRuntimeSetup,
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
  const [engineStatus, setEngineStatus] = useState<EngineUpdateStatusDto | null>(null);
  const [isUpdatingEngine, setIsUpdatingEngine] = useState(false);
  const [isRollingBackEngine, setIsRollingBackEngine] = useState(false);
  const [engineUpdateError, setEngineUpdateError] = useState<string | null>(null);
  const [engineUpdateDone, setEngineUpdateDone] = useState(false);
  const [healthError, setHealthError] = useState(false);

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
    setHealthError(false);
    try {
      const res = await defaultIpcClient.healthCheck();
      setHealth(res);
      // Best-effort engine update status; never blocks the drawer.
      try {
        const engine = await defaultIpcClient.checkEngineUpdate();
        setEngineStatus(engine);
      } catch {
        setEngineStatus(null);
      }
    } catch {
      // Distinguish "engine unavailable" from "could not read engine status".
      setHealthError(true);
    } finally {
      setLoadingHealth(false);
    }
  };

  const handleUpdateEngine = async () => {
    setIsUpdatingEngine(true);
    setEngineUpdateError(null);
    setEngineUpdateDone(false);
    try {
      const result = await defaultIpcClient.updateEngine();
      setEngineUpdateDone(true);
      if (!result.updated) {
        setEngineUpdateError(null);
      }
      await fetchHealth();
    } catch (err) {
      setEngineUpdateError(
        err instanceof Error ? err.message : t('engine.updateFailed'),
      );
    } finally {
      setIsUpdatingEngine(false);
    }
  };

  const handleRollbackEngine = async () => {
    setIsRollingBackEngine(true);
    setEngineUpdateError(null);
    setEngineUpdateDone(false);
    try {
      await defaultIpcClient.rollbackEngine();
      await fetchHealth();
    } catch (err) {
      setEngineUpdateError(
        err instanceof Error ? err.message : t('engine.rollbackFailed'),
      );
    } finally {
      setIsRollingBackEngine(false);
    }
  };

  const handleEngineChannelChange = (channel: EngineChannel) => {
    onUpdateSettings({ engineChannel: channel }, true);
    // Re-read the status against the newly selected channel.
    setTimeout(() => {
      void fetchHealth();
    }, 0);
  };

  const handleCookiesChange = (value: string) => {
    const browser = value === '' ? undefined : (value as CookiesBrowser);
    onUpdateSettings({ cookiesFromBrowser: browser }, true);
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
                  <LaptopIcon fontSize="small" />
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
                    <MenuItem value="p240">240p</MenuItem>
                    <MenuItem value="p144">144p</MenuItem>
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
            <Stack direction="row" spacing={0.5} alignItems="center">
              <Tooltip title={t('settings.diagnostics.refreshAria')}>
                <span>
                  <IconButton
                    size="small"
                    onClick={fetchHealth}
                    disabled={loadingHealth}
                    aria-label={t('settings.diagnostics.refreshAria')}
                  >
                    {loadingHealth ? (
                      <CircularProgress size={14} />
                    ) : (
                      <RefreshIcon fontSize="small" />
                    )}
                  </IconButton>
                </span>
              </Tooltip>
              <Tooltip title={t('engine.updateButton')}>
                <span>
                  <IconButton
                    size="small"
                    onClick={handleUpdateEngine}
                    disabled={isUpdatingEngine || loadingHealth}
                    aria-label={t('engine.updateButton')}
                  >
                    {isUpdatingEngine ? (
                      <CircularProgress size={14} />
                    ) : (
                      <UpgradeIcon fontSize="small" />
                    )}
                  </IconButton>
                </span>
              </Tooltip>
            </Stack>
          </Box>

          <Stack spacing={1}>
            {healthError && (
              <Alert severity="warning" onClose={() => setHealthError(false)}>
                {t('engine.diagnosticsFailed')}
              </Alert>
            )}
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
                    <Typography variant="caption" color="text.secondary" component="div">
                      {t('engine.currentVersion')} : v{health.ytdlp.version}
                    </Typography>
                  )}
                  {engineStatus?.latestVersion && (
                    <Typography variant="caption" color="text.secondary" component="div">
                      {t('engine.latestVersion')} : v{engineStatus.latestVersion}
                    </Typography>
                  )}
                </Box>
                <Stack direction="row" spacing={0.5} alignItems="center">
                  {engineStatus?.outdated && (
                    <Chip
                      size="small"
                      label={t('engine.outdatedTitle')}
                      color="warning"
                      variant="outlined"
                      sx={{ height: 22, fontSize: '0.75rem' }}
                    />
                  )}
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
                </Stack>
              </Box>
              {(engineStatus?.outdated || engineStatus?.canUpdate) && (
                <Box sx={{ mt: 1 }}>
                  <Button
                    fullWidth
                    size="small"
                    variant="outlined"
                    startIcon={
                      isUpdatingEngine ? (
                        <CircularProgress size={14} color="inherit" />
                      ) : (
                        <RefreshIcon fontSize="small" />
                      )
                    }
                    onClick={handleUpdateEngine}
                    disabled={isUpdatingEngine || isRollingBackEngine}
                    sx={{ textTransform: 'none', fontWeight: 600 }}
                  >
                    {isUpdatingEngine ? t('engine.updating') : t('engine.updateButton')}
                  </Button>
                </Box>
              )}

              {/* Release channel selector (stable / nightly) */}
              <Box sx={{ mt: 1.25 }}>
                <FormControl fullWidth size="small">
                  <InputLabel id="engine-channel-label">{t('engine.channelLabel')}</InputLabel>
                  <Select
                    labelId="engine-channel-label"
                    value={settings.engineChannel ?? 'stable'}
                    label={t('engine.channelLabel')}
                    onChange={(e) =>
                      handleEngineChannelChange(e.target.value as EngineChannel)
                    }
                  >
                    <MenuItem value="stable">{t('engine.channelStable')}</MenuItem>
                    <MenuItem value="nightly">{t('engine.channelNightly')}</MenuItem>
                  </Select>
                </FormControl>
                <Typography
                  variant="caption"
                  color="text.secondary"
                  sx={{ display: 'block', mt: 0.5, fontSize: '0.7rem', lineHeight: 1.35 }}
                >
                  {t('engine.channelHelp')}
                </Typography>
              </Box>

              {/* Rollback to the binary backed up before the last update */}
              {engineStatus?.canRollback && (
                <Box sx={{ mt: 1 }}>
                  <Button
                    fullWidth
                    size="small"
                    variant="text"
                    color="inherit"
                    onClick={handleRollbackEngine}
                    disabled={isUpdatingEngine || isRollingBackEngine}
                    sx={{ textTransform: 'none', fontWeight: 600, fontSize: '0.78rem' }}
                  >
                    {t('engine.rollbackButton')}
                  </Button>
                </Box>
              )}

              {engineUpdateDone && (
                <Alert severity="success" sx={{ mt: 1 }} onClose={() => setEngineUpdateDone(false)}>
                  {t('engine.upToDate')}
                </Alert>
              )}
              {engineUpdateError && (
                <Alert severity="error" sx={{ mt: 1 }} onClose={() => setEngineUpdateError(null)}>
                  {engineUpdateError}
                </Alert>
              )}
            </Card>

            {/* Cookie source: browser name only, no cookie data is ever read or stored */}
            <Card variant="outlined" sx={{ p: 1.25 }}>
              <FormControl fullWidth size="small">
                <InputLabel id="cookies-browser-label">{t('settings.cookies.label')}</InputLabel>
                <Select
                  labelId="cookies-browser-label"
                  value={settings.cookiesFromBrowser ?? ''}
                  label={t('settings.cookies.label')}
                  onChange={(e) => handleCookiesChange(e.target.value)}
                >
                  <MenuItem value="">{t('settings.cookies.none')}</MenuItem>
                  <MenuItem value="firefox">Firefox</MenuItem>
                  <MenuItem value="chrome">Chrome</MenuItem>
                  <MenuItem value="chromium">Chromium</MenuItem>
                  <MenuItem value="brave">Brave</MenuItem>
                  <MenuItem value="edge">Edge</MenuItem>
                  <MenuItem value="vivaldi">Vivaldi</MenuItem>
                  <MenuItem value="opera">Opera</MenuItem>
                  <MenuItem value="safari">Safari</MenuItem>
                  <MenuItem value="whale">Whale</MenuItem>
                </Select>
              </FormControl>
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{ display: 'block', mt: 0.75, fontSize: '0.7rem', lineHeight: 1.35 }}
              >
                {t('settings.cookies.helperText')}
              </Typography>
              {settings.cookiesFromBrowser === 'firefox' && (
                <Typography variant="caption" color="success.main" sx={{ display: 'block', mt: 0.5, fontSize: '0.7rem' }}>
                  {t('settings.cookies.firefoxHint')}
                </Typography>
              )}
              {settings.cookiesFromBrowser &&
                ['chrome', 'chromium', 'brave', 'edge', 'vivaldi'].includes(
                  settings.cookiesFromBrowser,
                ) && (
                  <Typography variant="caption" color="warning.main" sx={{ display: 'block', mt: 0.5, fontSize: '0.7rem' }}>
                    {t('settings.cookies.chromiumHint')}
                  </Typography>
                )}
              {settings.cookiesFromBrowser === 'safari' && (
                <Typography variant="caption" color="warning.main" sx={{ display: 'block', mt: 0.5, fontSize: '0.7rem' }}>
                  {t('settings.cookies.safariHint')}
                </Typography>
              )}
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

            {/* JavaScript runtime used by yt-dlp for YouTube challenges */}
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
                    {t('engine.jsRuntimeLabel')}
                  </Typography>
                  {health?.jsRuntime?.version ? (
                    <Typography variant="caption" color="text.secondary" component="div">
                      {t('jsRuntime.currentVersion')} : {health.jsRuntime.kind ?? ''}{' '}
                      {health.jsRuntime.version}
                    </Typography>
                  ) : (
                    <Typography variant="caption" color="text.secondary" component="div">
                      {t('engine.jsRuntimeMissing')}
                    </Typography>
                  )}
                </Box>
                <Stack direction="row" spacing={0.5} alignItems="center">
                  {!health?.jsRuntime?.isReady && (
                    <Button
                      size="small"
                      variant="outlined"
                      onClick={() => onOpenJsRuntimeSetup?.()}
                      sx={{ textTransform: 'none', fontWeight: 600, fontSize: '0.75rem' }}
                    >
                      {t('engine.jsRuntimeSetupButton')}
                    </Button>
                  )}
                  <Chip
                    size="small"
                    icon={
                      health?.jsRuntime?.isReady ? (
                        <CheckCircleOutlineIcon fontSize="small" />
                      ) : (
                        <ErrorOutlineIcon fontSize="small" />
                      )
                    }
                    label={
                      health?.jsRuntime?.isReady
                        ? t('settings.diagnostics.ready')
                        : t('settings.diagnostics.unavailable')
                    }
                    color={health?.jsRuntime?.isReady ? 'success' : 'warning'}
                    variant="outlined"
                    sx={{ height: 22, fontSize: '0.75rem' }}
                  />
                </Stack>
              </Box>
            </Card>
          </Stack>
        </Box>

        {errorMessage && <Alert severity="error">{errorMessage}</Alert>}
      </Stack>
    </Drawer>
  );
};

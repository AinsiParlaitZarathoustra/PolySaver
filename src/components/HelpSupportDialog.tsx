// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import React, { useState, useCallback } from 'react';
import {
  Dialog,
  DialogTitle,
  DialogContent,
  Tabs,
  Tab,
  Box,
  Typography,
  IconButton,
  Divider,
  Chip,
  Paper,
  Button,
  LinearProgress,
  Alert,
} from '@mui/material';
import CloseIcon from '@mui/icons-material/Close';
import ReportProblemOutlinedIcon from '@mui/icons-material/ReportProblemOutlined';
import LightbulbOutlinedIcon from '@mui/icons-material/LightbulbOutlined';
import MailOutlineIcon from '@mui/icons-material/MailOutline';
import ArrowOutwardIcon from '@mui/icons-material/ArrowOutward';
import SystemUpdateAltIcon from '@mui/icons-material/SystemUpdateAlt';
import CheckCircleOutlineIcon from '@mui/icons-material/CheckCircleOutline';
import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import { useTranslation } from 'react-i18next';
import type { IpcClient } from '../ipc/client';
import type { UpdateInfo } from '../ipc/contracts';

interface HelpSupportDialogProps {
  open: boolean;
  onClose: () => void;
  client: IpcClient;
  hasActiveDownloads: boolean;
}

export const HelpSupportDialog: React.FC<HelpSupportDialogProps> = ({
  open,
  onClose,
  client,
  hasActiveDownloads,
}) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<number>(0);

  // Updater state
  const [updaterStatus, setUpdaterStatus] = useState<
    'idle' | 'checking' | 'upToDate' | 'available' | 'downloading' | 'ready' | 'error'
  >('idle');
  const [availableUpdate, setAvailableUpdate] = useState<UpdateInfo | null>(null);
  const [downloadProgress, setDownloadProgress] = useState<number | null>(null);
  const [updaterError, setUpdaterError] = useState<string | null>(null);

  const handleTabChange = (_event: React.SyntheticEvent, newValue: number) => {
    setActiveTab(newValue);
  };

  const handleOpenSupport = useCallback(async () => {
    try {
      await client.openSupportPage();
    } catch (err) {
      console.error('Failed to open support page:', err);
    }
  }, [client]);

  const handleCheckForUpdates = useCallback(async () => {
    setUpdaterStatus('checking');
    setUpdaterError(null);
    try {
      const update = await client.checkForUpdates();
      if (update) {
        setAvailableUpdate(update);
        setUpdaterStatus('available');
      } else {
        setAvailableUpdate(null);
        setUpdaterStatus('upToDate');
      }
    } catch (err) {
      console.error('Failed to check for updates:', err);
      setUpdaterError(err instanceof Error ? err.message : t('help.updater.error'));
      setUpdaterStatus('error');
    }
  }, [client, t]);

  const handleInstallUpdate = useCallback(async () => {
    if (hasActiveDownloads) return;
    setUpdaterStatus('downloading');
    setDownloadProgress(0);
    try {
      await client.downloadAndInstallUpdate((downloaded, total) => {
        if (total && total > 0) {
          setDownloadProgress(Math.round((downloaded / total) * 100));
        } else {
          setDownloadProgress(null);
        }
      });
      setUpdaterStatus('ready');
    } catch (err) {
      console.error('Failed to install update:', err);
      setUpdaterError(err instanceof Error ? err.message : t('help.updater.error'));
      setUpdaterStatus('error');
    }
  }, [client, hasActiveDownloads, t]);

  const handleRestart = useCallback(async () => {
    try {
      await client.restartApp();
    } catch (err) {
      console.error('Failed to restart app:', err);
    }
  }, [client]);

  return (
    <Dialog
      open={open}
      onClose={onClose}
      maxWidth="md"
      fullWidth
      aria-labelledby="help-support-dialog-title"
      PaperProps={{
        sx: {
          borderRadius: 3,
          maxHeight: '85vh',
          display: 'flex',
          flexDirection: 'column',
        },
      }}
    >
      <DialogTitle
        id="help-support-dialog-title"
        sx={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-start',
          pb: 1,
          px: 3,
          pt: 2.5,
        }}
      >
        <Box>
          <Typography variant="h6" component="h2" sx={{ fontWeight: 700 }}>
            {t('help.title')}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {t('help.subtitle')}
          </Typography>
        </Box>
        <IconButton
          onClick={onClose}
          aria-label={t('help.closeAria')}
          size="small"
          sx={{ color: 'text.secondary' }}
        >
          <CloseIcon />
        </IconButton>
      </DialogTitle>

      <Box sx={{ borderBottom: 1, borderColor: 'divider', px: 3 }}>
        <Tabs
          value={activeTab}
          onChange={handleTabChange}
          aria-label={t('help.title')}
          textColor="primary"
          indicatorColor="primary"
        >
          <Tab label={t('help.tabs.appGuide')} id="help-tab-0" aria-controls="help-tabpanel-0" />
          <Tab label={t('help.tabs.formatGuide')} id="help-tab-1" aria-controls="help-tabpanel-1" />
          <Tab label={t('help.tabs.support')} id="help-tab-2" aria-controls="help-tabpanel-2" />
        </Tabs>
      </Box>

      <DialogContent sx={{ px: 3, py: 2.5, overflowY: 'auto' }}>
        {/* Tab 0: App Guide */}
        {activeTab === 0 && (
          <Box role="tabpanel" id="help-tabpanel-0" aria-labelledby="help-tab-0" sx={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
            {/* Quick download */}
            <Box>
              <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 1 }}>
                {t('help.appGuide.quickDownloadTitle')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 0.5 }}>
                {t('help.appGuide.quickDownloadStep1')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 0.5 }}>
                {t('help.appGuide.quickDownloadStep2')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 1.5 }}>
                {t('help.appGuide.quickDownloadStep3')}
              </Typography>
              <Paper
                variant="outlined"
                sx={{
                  p: 1.5,
                  borderRadius: 2,
                  bgcolor: (theme) =>
                    theme.palette.mode === 'dark'
                      ? 'rgba(255,255,255,0.03)'
                      : 'rgba(0,0,0,0.02)',
                  borderColor: 'divider',
                }}
              >
                <Typography variant="body2" color="text.secondary" sx={{ fontStyle: 'italic' }}>
                  {t('help.appGuide.quickDownloadNote')}
                </Typography>
              </Paper>
            </Box>

            <Divider />

            {/* Custom download */}
            <Box>
              <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 1 }}>
                {t('help.appGuide.customDownloadTitle')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 0.5 }}>
                {t('help.appGuide.customDownloadStep1')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 0.5 }}>
                {t('help.appGuide.customDownloadStep2')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 0.5 }}>
                {t('help.appGuide.customDownloadStep3')}
              </Typography>
              <Typography variant="body2" color="text.primary" sx={{ mb: 1.5 }}>
                {t('help.appGuide.customDownloadStep4')}
              </Typography>
              <Paper
                variant="outlined"
                sx={{
                  p: 1.5,
                  borderRadius: 2,
                  bgcolor: (theme) =>
                    theme.palette.mode === 'dark'
                      ? 'rgba(255,255,255,0.03)'
                      : 'rgba(0,0,0,0.02)',
                  borderColor: 'divider',
                }}
              >
                <Typography variant="body2" color="text.secondary" sx={{ fontStyle: 'italic' }}>
                  {t('help.appGuide.customDownloadNote')}
                </Typography>
              </Paper>
            </Box>
          </Box>
        )}

        {/* Tab 1: Format Guide */}
        {activeTab === 1 && (
          <Box role="tabpanel" id="help-tabpanel-1" aria-labelledby="help-tab-1" sx={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
            {/* Video qualities */}
            <Box>
              <Typography variant="h6" sx={{ fontWeight: 700, mb: 2 }}>
                {t('help.formatGuide.videoQualitiesTitle')}
              </Typography>

              <Box sx={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                {/* 144p - 360p */}
                <Box>
                  <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                    {t('help.formatGuide.quality144_360Title')}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality144_360Desc')}
                  </Typography>
                </Box>

                {/* 480p */}
                <Box>
                  <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                    {t('help.formatGuide.quality480Title')}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality480Desc')}
                  </Typography>
                </Box>

                {/* 720p */}
                <Box>
                  <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                    {t('help.formatGuide.quality720Title')}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality720Desc')}
                  </Typography>
                </Box>

                {/* 1080p */}
                <Box>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 0.25 }}>
                    <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                      {t('help.formatGuide.quality1080Title')}
                    </Typography>
                    <Chip
                      label={t('help.formatGuide.recommendedBadge')}
                      size="small"
                      color="primary"
                      variant="outlined"
                      sx={{ height: 20, fontSize: '0.7rem', fontWeight: 600 }}
                    />
                  </Box>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality1080Desc')}
                  </Typography>
                </Box>

                {/* 1440p */}
                <Box>
                  <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                    {t('help.formatGuide.quality1440Title')}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality1440Desc')}
                  </Typography>
                </Box>

                {/* 2160p */}
                <Box>
                  <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                    {t('help.formatGuide.quality2160Title')}
                  </Typography>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.quality2160Desc')}
                  </Typography>
                </Box>

                {/* Above 4k */}
                <Box>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 0.25 }}>
                    <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                      {t('help.formatGuide.qualityAbove4kTitle')}
                    </Typography>
                    <Chip
                      label={t('help.formatGuide.notRecommendedBadge')}
                      size="small"
                      color="default"
                      variant="outlined"
                      sx={{ height: 20, fontSize: '0.7rem', fontWeight: 600 }}
                    />
                  </Box>
                  <Typography variant="body2" color="text.secondary">
                    {t('help.formatGuide.qualityAbove4kDesc')}
                  </Typography>
                </Box>
              </Box>
            </Box>

            <Divider />

            {/* Audio & Video Formats */}
            <Box>
              <Typography variant="h6" sx={{ fontWeight: 700, mb: 2 }}>
                {t('help.formatGuide.audioVideoFormatsTitle')}
              </Typography>

              {/* MP4 or MOV */}
              <Box sx={{ mb: 2.5 }}>
                <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 0.5 }}>
                  {t('help.formatGuide.mp4OrMovTitle')}
                </Typography>
                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  {t('help.formatGuide.mp4OrMovDesc1')}
                </Typography>
                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  {t('help.formatGuide.mp4OrMovDesc2')}
                </Typography>
                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  {t('help.formatGuide.mp4OrMovDesc3')}
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  {t('help.formatGuide.mp4OrMovDesc4')}
                </Typography>
              </Box>

              {/* MP3 or FLAC */}
              <Box>
                <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 0.5 }}>
                  {t('help.formatGuide.mp3OrFlacTitle')}
                </Typography>
                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  {t('help.formatGuide.mp3OrFlacDesc1')}
                </Typography>
                <Typography variant="body2" color="text.secondary" sx={{ mb: 0.5 }}>
                  {t('help.formatGuide.mp3OrFlacDesc2')}
                </Typography>
                <Typography variant="body2" color="text.secondary">
                  {t('help.formatGuide.mp3OrFlacDesc3')}
                </Typography>
              </Box>
            </Box>
          </Box>
        )}

        {/* Tab 2: Support & Updates */}
        {activeTab === 2 && (
          <Box role="tabpanel" id="help-tabpanel-2" aria-labelledby="help-tab-2" sx={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
            {/* Support Actions */}
            <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1.5 }}>
              {/* Report Issue */}
              <Paper
                variant="outlined"
                onClick={handleOpenSupport}
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    void handleOpenSupport();
                  }
                }}
                aria-label={t('help.support.reportIssueTitle')}
                sx={{
                  p: 2,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  cursor: 'pointer',
                  borderRadius: 2,
                  transition: 'background-color 0.2s',
                  '&:hover': {
                    bgcolor: (theme) =>
                      theme.palette.mode === 'dark'
                        ? 'rgba(255,255,255,0.05)'
                        : 'rgba(0,0,0,0.03)',
                  },
                }}
              >
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                  <ReportProblemOutlinedIcon color="action" />
                  <Box>
                    <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                      {t('help.support.reportIssueTitle')}
                    </Typography>
                    <Typography variant="body2" color="text.secondary">
                      {t('help.support.reportIssueDesc')}
                    </Typography>
                  </Box>
                </Box>
                <ArrowOutwardIcon fontSize="small" color="action" />
              </Paper>

              {/* Make Suggestion */}
              <Paper
                variant="outlined"
                onClick={handleOpenSupport}
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    void handleOpenSupport();
                  }
                }}
                aria-label={t('help.support.makeSuggestionTitle')}
                sx={{
                  p: 2,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  cursor: 'pointer',
                  borderRadius: 2,
                  transition: 'background-color 0.2s',
                  '&:hover': {
                    bgcolor: (theme) =>
                      theme.palette.mode === 'dark'
                        ? 'rgba(255,255,255,0.05)'
                        : 'rgba(0,0,0,0.03)',
                  },
                }}
              >
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                  <LightbulbOutlinedIcon color="action" />
                  <Box>
                    <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                      {t('help.support.makeSuggestionTitle')}
                    </Typography>
                    <Typography variant="body2" color="text.secondary">
                      {t('help.support.makeSuggestionDesc')}
                    </Typography>
                  </Box>
                </Box>
                <ArrowOutwardIcon fontSize="small" color="action" />
              </Paper>

              {/* Contact Developer */}
              <Paper
                variant="outlined"
                onClick={handleOpenSupport}
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    void handleOpenSupport();
                  }
                }}
                aria-label={t('help.support.contactMeTitle')}
                sx={{
                  p: 2,
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  cursor: 'pointer',
                  borderRadius: 2,
                  transition: 'background-color 0.2s',
                  '&:hover': {
                    bgcolor: (theme) =>
                      theme.palette.mode === 'dark'
                        ? 'rgba(255,255,255,0.05)'
                        : 'rgba(0,0,0,0.03)',
                  },
                }}
              >
                <Box sx={{ display: 'flex', alignItems: 'center', gap: 2 }}>
                  <MailOutlineIcon color="action" />
                  <Box>
                    <Typography variant="subtitle2" sx={{ fontWeight: 700 }}>
                      {t('help.support.contactMeTitle')}
                    </Typography>
                    <Typography variant="body2" color="text.secondary">
                      {t('help.support.contactMeDesc')}
                    </Typography>
                  </Box>
                </Box>
                <ArrowOutwardIcon fontSize="small" color="action" />
              </Paper>
            </Box>

            <Divider />

            {/* Software Updates */}
            <Box>
              <Typography variant="subtitle1" sx={{ fontWeight: 700, mb: 1 }}>
                {t('help.updater.title')}
              </Typography>

              <Box sx={{ display: 'flex', alignItems: 'center', gap: 2, flexWrap: 'wrap', mb: 2 }}>
                <Button
                  variant="outlined"
                  startIcon={<SystemUpdateAltIcon />}
                  onClick={handleCheckForUpdates}
                  disabled={updaterStatus === 'checking' || updaterStatus === 'downloading'}
                >
                  {updaterStatus === 'checking'
                    ? t('help.updater.checking')
                    : t('help.updater.checkForUpdates')}
                </Button>

                {updaterStatus === 'upToDate' && (
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, color: 'success.main' }}>
                    <CheckCircleOutlineIcon fontSize="small" />
                    <Typography variant="body2" sx={{ fontWeight: 500 }}>
                      {t('help.updater.upToDate')}
                    </Typography>
                  </Box>
                )}

                {updaterStatus === 'error' && (
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 0.5, color: 'error.main' }}>
                    <ErrorOutlineIcon fontSize="small" />
                    <Typography variant="body2" sx={{ fontWeight: 500 }}>
                      {updaterError ?? t('help.updater.error')}
                    </Typography>
                  </Box>
                )}
              </Box>

              {/* Update Available Card */}
              {updaterStatus === 'available' && availableUpdate && (
                <Paper
                  variant="outlined"
                  sx={{
                    p: 2,
                    borderRadius: 2,
                    borderColor: 'primary.main',
                    bgcolor: (theme) =>
                      theme.palette.mode === 'dark'
                        ? 'rgba(25, 118, 210, 0.08)'
                        : 'rgba(25, 118, 210, 0.04)',
                  }}
                >
                  <Typography variant="subtitle2" sx={{ fontWeight: 700, mb: 0.5 }}>
                    {t('help.updater.updateAvailable', { version: availableUpdate.version })}
                  </Typography>
                  <Typography variant="body2" color="text.secondary" sx={{ mb: 1.5 }}>
                    {t('help.updater.updateAvailablePrompt')}
                  </Typography>

                  {hasActiveDownloads && (
                    <Alert severity="warning" sx={{ mb: 1.5 }}>
                      {t('help.updater.activeDownloadsWarning')}
                    </Alert>
                  )}

                  <Box sx={{ display: 'flex', gap: 1 }}>
                    <Button
                      variant="contained"
                      onClick={handleInstallUpdate}
                      disabled={hasActiveDownloads}
                    >
                      {t('help.updater.updateNow')}
                    </Button>
                  </Box>
                </Paper>
              )}

              {/* Downloading Progress */}
              {updaterStatus === 'downloading' && (
                <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
                  <Typography variant="body2" sx={{ fontWeight: 600, mb: 1 }}>
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
                </Paper>
              )}

              {/* Ready to restart */}
              {updaterStatus === 'ready' && (
                <Paper
                  variant="outlined"
                  sx={{
                    p: 2,
                    borderRadius: 2,
                    borderColor: 'success.main',
                    bgcolor: (theme) =>
                      theme.palette.mode === 'dark'
                        ? 'rgba(46, 125, 50, 0.08)'
                        : 'rgba(46, 125, 50, 0.04)',
                  }}
                >
                  <Typography variant="subtitle2" sx={{ fontWeight: 700, mb: 0.5 }}>
                    {t('help.updater.updateInstalled')}
                  </Typography>
                  <Button variant="contained" color="success" onClick={handleRestart} sx={{ mt: 1 }}>
                    {t('help.updater.restartNow')}
                  </Button>
                </Paper>
              )}
            </Box>
          </Box>
        )}
      </DialogContent>
    </Dialog>
  );
};

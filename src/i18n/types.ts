// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import type { DownloadErrorCode } from '../ipc/contracts';

export interface TranslationSchema {
  common: {
    appName: string;
    cancel: string;
    confirm: string;
    close: string;
    retry: string;
    browse: string;
    loading: string;
    error: string;
    success: string;
    video: string;
    audio: string;
    format: string;
    quality: string;
    status: string;
    speed: string;
    eta: string;
  };
  header: {
    tagline: string;
    preferencesButtonAria: string;
    preferencesTooltip: string;
    helpButtonAria: string;
    helpTooltip: string;
  };
  form: {
    urlLabel: string;
    urlPlaceholder: string;
    clearButtonAria: string;
    quickDownloadButton: string;
    quickDownloadTooltip: string;
    downloadButton: string;
    downloadTooltip: string;
    analyzing: string;
    invalidUrlError: string;
    playlistNotSupportedError: string;
    emptyUrlError: string;
  };
  dialog: {
    title: string;
    subtitle: string;
    authorPrefix: string;
    durationPrefix: string;
    typeSelector: string;
    formatSelector: string;
    qualitySelector: string;
    bestQuality: string;
    losslessQuality: string;
    bitrateSuffix: string;
    startDownload: string;
    analyzingPreview: string;
    chooseLocation: string;
    selectedFolder: string;
    chooseLocationAria: string;
  };
  queue: {
    title: string;
    emptyTitle: string;
    emptySubtitle: string;
    itemCount_one: string;
    itemCount_other: string;
    status: {
      queued: string;
      preparing: string;
      probing: string;
      downloading: string;
      converting: string;
      finalizing: string;
      completed: string;
      failed: string;
      canceled: string;
    };
    actions: {
      cancelAria: string;
      retryAria: string;
      showInFolder: string;
      openFile: string;
      showInFolderAria: string;
      openFileAria: string;
      dismissAria: string;
      openSourceUrlAria: string;
    };
    progress: {
      remaining: string;
      calculating: string;
      complete: string;
    };
  };
  history: {
    title: string;
    itemCount_one: string;
    itemCount_other: string;
    removeFromHistory: string;
    removeFromHistoryAria: string;
  };
  settings: {
    title: string;
    subtitle: string;
    closeAria: string;
    theme: {
      label: string;
      light: string;
      dark: string;
      system: string;
      lightAria: string;
      darkAria: string;
      systemAria: string;
    };
    language: {
      label: string;
      fr: string;
      en: string;
      frAria: string;
      enAria: string;
    };
    directory: {
      label: string;
      placeholder: string;
      browseButton: string;
      browseAria: string;
      browseError: string;
      helperText: string;
    };
    defaultPreset: {
      label: string;
      helperText: string;
    };
    concurrency: {
      label: string;
      toggleLabel: string;
      toggleDescription: string;
      sliderLabel: string;
    };
    diagnostics: {
      label: string;
      coreLabel: string;
      ytdlpLabel: string;
      ffmpegLabel: string;
      ready: string;
      unavailable: string;
      unknown: string;
    };
    autosave: {
      saving: string;
      saved: string;
      error: string;
    };
  };
  help: {
    title: string;
    subtitle: string;
    closeAria: string;
    tabs: {
      appGuide: string;
      formatGuide: string;
      support: string;
    };
    appGuide: {
      quickDownloadTitle: string;
      quickDownloadStep1: string;
      quickDownloadStep2: string;
      quickDownloadStep3: string;
      quickDownloadNote: string;
      customDownloadTitle: string;
      customDownloadStep1: string;
      customDownloadStep2: string;
      customDownloadStep3: string;
      customDownloadStep4: string;
      customDownloadNote: string;
    };
    formatGuide: {
      videoQualitiesTitle: string;
      audioVideoFormatsTitle: string;
      recommendedBadge: string;
      notRecommendedBadge: string;
      quality144_360Title: string;
      quality144_360Desc: string;
      quality480Title: string;
      quality480Desc: string;
      quality720Title: string;
      quality720Desc: string;
      quality1080Title: string;
      quality1080Desc: string;
      quality1440Title: string;
      quality1440Desc: string;
      quality2160Title: string;
      quality2160Desc: string;
      qualityAbove4kTitle: string;
      qualityAbove4kDesc: string;
      mp4OrMovTitle: string;
      mp4OrMovDesc1: string;
      mp4OrMovDesc2: string;
      mp4OrMovDesc3: string;
      mp4OrMovDesc4: string;
      mp3OrFlacTitle: string;
      mp3OrFlacDesc1: string;
      mp3OrFlacDesc2: string;
      mp3OrFlacDesc3: string;
    };
    support: {
      reportIssueTitle: string;
      reportIssueDesc: string;
      makeSuggestionTitle: string;
      makeSuggestionDesc: string;
      contactMeTitle: string;
      contactMeDesc: string;
      actionAria: string;
    };
    updater: {
      title: string;
      checkForUpdates: string;
      checking: string;
      upToDate: string;
      updateAvailable: string;
      updateAvailablePrompt: string;
      updateNow: string;
      updateLater: string;
      downloadingUpdate: string;
      activeDownloadsWarning: string;
      restartNow: string;
      updateInstalled: string;
      error: string;
    };
  };
  errors: Record<DownloadErrorCode, string> & {
    UNKNOWN_ERROR: string;
  };
}

export type LocaleResource = TranslationSchema;

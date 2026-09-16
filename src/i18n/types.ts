// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import type { DownloadErrorCode } from '../ipc/contracts';

export interface TranslationSchema {
  common: {
    appName: string;
    cancel: string;
    close: string;
    video: string;
    audio: string;
    format: string;
    quality: string;
  };
  header: {
    preferencesButtonAria: string;
    preferencesTooltip: string;
    helpButtonAria: string;
    helpTooltip: string;
  };
  form: {
    urlPlaceholder: string;
    quickDownloadButton: string;
    quickDownloadTooltip: string;
    downloadButton: string;
    downloadTooltip: string;
    invalidUrlError: string;
    emptyUrlError: string;
    playlistModeBadge: string;
    playlistFastUnavailable: string;
  };
  dialog: {
    title: string;
    typeSelector: string;
    formatSelector: string;
    qualitySelector: string;
    bestQuality: string;
    losslessQuality: string;
    startDownload: string;
    chooseLocation: string;
    chooseLocationAria: string;
    playlistVideosTitle: string;
    playlistSelectAll: string;
    playlistSelectNone: string;
    playlistSelectedCount: string;
    playlistTruncated: string;
    playlistEmpty: string;
    playlistUnavailableEntry: string;
    playlistConfirm: string;
    playlistNoSelection: string;
  };
  queue: {
    title: string;
    emptyTitle: string;
    emptySubtitle: string;
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
    };
    retryAttempt: string;
    errorDetails: string;
    errorDetailsComponent: string;
    errorDetailsExitCode: string;
    playlistQueued: string;
  };
  history: {
    title: string;
    removeFromHistory: string;
    removeFromHistoryAria: string;
  };
  settings: {
    title: string;
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
    };
    defaultPreset: {
      label: string;
    };
    cookies: {
      label: string;
      none: string;
      helperText: string;
      firefoxHint: string;
      chromiumHint: string;
      safariHint: string;
    };
    diagnostics: {
      label: string;
      ready: string;
      unavailable: string;
      refreshAria: string;
    };
    autosave: {
      saving: string;
      saved: string;
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
      quickDownloadStep3Prefix: string;
      quickDownloadStep3Link: string;
      customDownloadTitle: string;
      customDownloadStep1: string;
      customDownloadStep2: string;
      customDownloadStep3: string;
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
  engine: {
    outdatedTitle: string;
    outdatedMessage: string;
    updateButton: string;
    updating: string;
    updateFailed: string;
    upToDate: string;
    currentVersion: string;
    latestVersion: string;
    channelLabel: string;
    channelStable: string;
    channelNightly: string;
    channelHelp: string;
    rollbackButton: string;
    rollbackFailed: string;
    diagnosticsFailed: string;
    jsRuntimeLabel: string;
    jsRuntimeMissing: string;
    jsRuntimeSetupButton: string;
  };
  jsRuntime: {
    title: string;
    description: string;
    installButton: string;
    laterButton: string;
    installing: string;
    installFailed: string;
    installed: string;
    currentVersion: string;
  };
}

export type LocaleResource = TranslationSchema;

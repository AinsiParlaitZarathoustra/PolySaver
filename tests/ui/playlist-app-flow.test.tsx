// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// The app shell subscribes to Tauri events on mount; the browser test has no IPC
// bridge, so the event module is replaced by inert listeners. Plain functions are
// used on purpose: `vi.restoreAllMocks()` in `afterEach` would reset `vi.fn()`
// implementations and break the unmount cleanup. The factory is inlined because
// `vi.mock` is hoisted above any top-level declaration.
vi.mock('../../src/ipc/events', () => {
  const noopUnlisten = async (): Promise<() => void> => () => {};
  return {
    onDownloadProgress: noopUnlisten,
    onDownloadQueued: noopUnlisten,
    onDownloadCompleted: noopUnlisten,
    onDownloadFailed: noopUnlisten,
    onDownloadCanceled: noopUnlisten,
    onDownloadWarning: noopUnlisten,
  };
});

import '../../src/i18n';
import { setAppLanguage } from '../../src/i18n';
import { App } from '../../src/app/App';
import { defaultIpcClient } from '../../src/ipc/client';
import type { AppSettingsDto, DownloadJobDto, ProbeResult } from '../../src/ipc/contracts';

const settings: AppSettingsDto = {
  downloadDirectory: '~/Downloads/PolySaver',
  themeMode: 'system',
  defaultPreset: { format: 'mp4', videoQuality: 'p1080' },
  language: 'fr',
};

const playlistProbe: ProbeResult = {
  url: 'https://www.youtube.com/playlist?list=PL42',
  title: 'Ma playlist',
  thumbnailUrl: 'https://i.ytimg.com/vi/first111/hqdefault.jpg',
  uploader: 'Une chaîne',
  formats: [],
  availableVideoQualities: [],
  kind: 'playlist',
  entries: [
    {
      index: 1,
      url: 'https://www.youtube.com/watch?v=first111',
      title: 'Première vidéo',
      durationSeconds: 61,
      available: true,
    },
    {
      index: 2,
      url: 'https://www.youtube.com/watch?v=second222',
      title: 'Deuxième vidéo',
      durationSeconds: 125,
      available: true,
    },
  ],
  playlistTotal: 2,
  entriesLimit: 200,
};

function job(id: string, url: string): DownloadJobDto {
  return {
    id,
    url,
    preset: { format: 'mp4', videoQuality: 'p1080' },
    title: null,
    status: 'queued',
    progressPercent: null,
    retryCount: 0,
  };
}

/**
 * These two tests drive the whole app shell (typing a URL, opening the dialog,
 * checking entries). On a loaded or slow runner that exceeds vitest's default 5 s,
 * so the budget is raised here rather than globally.
 */
describe('Sprint 5: playlist mode end-to-end (App wiring)', () => {
  beforeEach(async () => {
    await setAppLanguage('fr');
    vi.spyOn(defaultIpcClient, 'getSettings').mockResolvedValue(settings);
    vi.spyOn(defaultIpcClient, 'listDownloads').mockResolvedValue([]);
    vi.spyOn(defaultIpcClient, 'listDownloadHistory').mockResolvedValue([]);
    vi.spyOn(defaultIpcClient, 'healthCheck').mockResolvedValue({
      coreStatus: 'ok',
      ytdlp: { isReady: true, statusMessage: 'ok' },
      ffmpeg: { isReady: true, statusMessage: 'ok' },
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('queues one job per checked video and reports the batch size', async () => {
    const user = userEvent.setup();
    vi.spyOn(defaultIpcClient, 'analyzeUrl').mockResolvedValue(playlistProbe);
    const startPlaylistDownload = vi
      .spyOn(defaultIpcClient, 'startPlaylistDownload')
      .mockResolvedValue([
        job('job-1', 'https://www.youtube.com/watch?v=first111'),
        job('job-2', 'https://www.youtube.com/watch?v=second222'),
      ]);
    const startDownload = vi.spyOn(defaultIpcClient, 'startDownload');

    render(<App />);

    const input = await screen.findByPlaceholderText(/collez un lien ici/i);
    await user.type(input, 'https://www.youtube.com/playlist?list=PL42');

    // Playlist mode: the green button is unavailable, the blue one opens the dialog.
    expect(screen.getByText('Mode playlist')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /téléchargement rapide/i })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: /^télécharger$/i }));

    await screen.findByText('Vidéos de la playlist');
    await user.click(screen.getByRole('button', { name: /tout sélectionner/i }));
    await user.click(screen.getByRole('button', { name: /lancer le téléchargement \(2\)/i }));

    await waitFor(() => {
      expect(startPlaylistDownload).toHaveBeenCalledWith(
        'https://www.youtube.com/playlist?list=PL42',
        { format: 'mp4', videoQuality: 'p1080' },
        undefined,
        [
          'https://www.youtube.com/watch?v=first111',
          'https://www.youtube.com/watch?v=second222',
        ],
      );
    });

    // The single-video pipeline is never used for a playlist.
    expect(startDownload).not.toHaveBeenCalled();

    // Both videos show up individually in the queue, with a confirmation toast.
    expect(await screen.findByText('Première vidéo')).toBeInTheDocument();
    expect(screen.getByText('2 vidéos ajoutées à la file')).toBeInTheDocument();
  }, 20000);

  it('surfaces a batch failure instead of leaving it silent', async () => {
    const user = userEvent.setup();
    vi.spyOn(defaultIpcClient, 'analyzeUrl').mockResolvedValue(playlistProbe);
    vi.spyOn(defaultIpcClient, 'startPlaylistDownload').mockRejectedValue(
      new Error('Refus du moteur: sélection invalide'),
    );

    render(<App />);

    const input = await screen.findByPlaceholderText(/collez un lien ici/i);
    await user.type(input, 'https://www.youtube.com/playlist?list=PL42');
    await user.click(screen.getByRole('button', { name: /^télécharger$/i }));

    await screen.findByText('Vidéos de la playlist');
    await user.click(screen.getByRole('button', { name: /tout sélectionner/i }));
    await user.click(screen.getByRole('button', { name: /lancer le téléchargement \(2\)/i }));

    expect(await screen.findByText('Refus du moteur: sélection invalide')).toBeInTheDocument();
  }, 20000);
});

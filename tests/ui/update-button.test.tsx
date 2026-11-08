// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// The app shell subscribes to Tauri events on mount; the test has no IPC
// bridge (same inert listeners as the other app-level tests).
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
import { ThemeProvider } from '@mui/material';
import '../../src/i18n';
import { setAppLanguage } from '../../src/i18n';
import { createAppTheme } from '../../src/app/theme';
import { Header } from '../../src/components/Header';
import { UpdatePromptDialog } from '../../src/components/UpdatePromptDialog';
import { App } from '../../src/app/App';
import { defaultIpcClient, type IpcClient } from '../../src/ipc/client';
import { extractWhatsNew } from '../../src/utils/releaseNotes';
import type { AppSettingsDto, UpdateInfo } from '../../src/ipc/contracts';

/** Shape of the real 2.6.0 release text: screenshots, then the download guide. */
const RELEASE_BODY = [
  '## PolySaver 2.7.0: Playlists done right, smarter downloads',
  '',
  '### 🎵 Playlists, reimagined',
  '- **One playlist, one tidy folder.** Videos numbered in playlist order.',
  '- **More reliable detection.** TikTok videos finally download.',
  '',
  '### ✨ Little things that matter',
  '- **Sizes in your language.** *Mo* and *Go* in French.',
  '',
  '<img width="1072" height="792" alt="Capture d’écran" src="https://github.com/user-attachments/assets/aaa" />',
  '![screenshot](https://github.com/user-attachments/assets/bbb)',
  '',
  '---',
  '',
  '### ⬇️ Téléchargement',
  '',
  '- Windows : PolySaver_2.7.0_Windows_x64_Setup.exe',
].join('\n');

const update: UpdateInfo = { version: '2.7.0', currentVersion: '2.6.0' };

const withTheme = (node: React.ReactNode) => (
  <ThemeProvider theme={createAppTheme('light')}>{node}</ThemeProvider>
);

describe('extractWhatsNew', () => {
  it('keeps only what is new: no image, nothing from the download section on', () => {
    const { title, blocks } = extractWhatsNew(RELEASE_BODY);

    expect(title).toBe('PolySaver 2.7.0: Playlists done right, smarter downloads');
    const text = JSON.stringify(blocks);
    expect(text).toContain('One playlist, one tidy folder.');
    expect(text).toContain('Little things that matter');
    expect(text).not.toMatch(/img|user-attachments|screenshot|Téléchargement|Setup\.exe/);
    expect(blocks.map((b) => b.kind)).toEqual([
      'heading',
      'bullet',
      'bullet',
      'heading',
      'bullet',
    ]);
    expect(blocks[1].spans[0]).toEqual({ text: 'One playlist, one tidy folder.', bold: true });
  });

  it('cuts only at a section that starts with the download guide', () => {
    const { blocks } = extractWhatsNew(
      '## PolySaver 3.0: faster downloads\n### Downloads, faster\n- **Quick.** Yes.\n### 📥 Download\n- Setup.exe',
    );
    const text = JSON.stringify(blocks);
    expect(text).toContain('Downloads, faster');
    expect(text).toContain('Quick.');
    expect(text).not.toContain('Setup.exe');
  });

  it('copes with an empty or missing release text', () => {
    expect(extractWhatsNew(null)).toEqual({ title: null, blocks: [] });
    expect(extractWhatsNew('')).toEqual({ title: null, blocks: [] });
  });
});

describe('Header update button', () => {
  beforeEach(async () => {
    await setAppLanguage('fr');
  });

  it('only appears when an update is available, left of the help button', async () => {
    const user = userEvent.setup();
    const onOpenUpdate = vi.fn();
    const { rerender } = render(
      withTheme(<Header onOpenSettings={vi.fn()} onOpenHelp={vi.fn()} onOpenUpdate={onOpenUpdate} />),
    );
    expect(screen.queryByRole('button', { name: /mise à jour disponible/i })).not.toBeInTheDocument();

    rerender(
      withTheme(
        <Header
          onOpenSettings={vi.fn()}
          onOpenHelp={vi.fn()}
          updateVersion="2.7.0"
          onOpenUpdate={onOpenUpdate}
        />,
      ),
    );
    const buttons = screen.getAllByRole('button');
    const updateButton = screen.getByRole('button', {
      name: 'Mise à jour disponible : PolySaver 2.7.0',
    });
    expect(buttons.indexOf(updateButton)).toBeLessThan(
      buttons.indexOf(screen.getByRole('button', { name: 'Aide et support' })),
    );
    expect(screen.getByTestId('ArrowDownwardIcon')).toBeInTheDocument();

    await user.click(updateButton);
    expect(onOpenUpdate).toHaveBeenCalledTimes(1);
  });
});

describe('Update dialog', () => {
  beforeEach(async () => {
    await setAppLanguage('fr');
  });

  it('shows the release name and what is new, and still installs the update', async () => {
    const user = userEvent.setup();
    const downloadAndInstallUpdate = vi.fn().mockResolvedValue(undefined);
    const client = {
      getReleaseNotes: vi.fn().mockResolvedValue({ name: 'PolySaver 2.7.0', body: RELEASE_BODY }),
      downloadAndInstallUpdate,
      restartApp: vi.fn(),
    } as unknown as IpcClient;

    render(
      withTheme(
        <UpdatePromptDialog
          open={true}
          onClose={vi.fn()}
          updateInfo={update}
          client={client}
          hasActiveDownloads={false}
        />,
      ),
    );

    expect(
      await screen.findByText('PolySaver 2.7.0: Playlists done right, smarter downloads'),
    ).toBeInTheDocument();
    expect(screen.getByText('One playlist, one tidy folder.')).toBeInTheDocument();
    expect(screen.queryByText(/Téléchargement/)).not.toBeInTheDocument();
    expect(document.querySelector('img')).toBeNull();
    expect(client.getReleaseNotes).toHaveBeenCalledWith('2.7.0');

    await user.click(screen.getByRole('button', { name: 'Mettre à jour' }));
    await waitFor(() => expect(downloadAndInstallUpdate).toHaveBeenCalled());
  });

  it('still offers the update when the release text cannot be fetched', async () => {
    const client = {
      getReleaseNotes: vi.fn().mockRejectedValue(new Error('offline')),
      downloadAndInstallUpdate: vi.fn(),
      restartApp: vi.fn(),
    } as unknown as IpcClient;

    render(
      withTheme(
        <UpdatePromptDialog
          open={true}
          onClose={vi.fn()}
          updateInfo={update}
          client={client}
          hasActiveDownloads={false}
        />,
      ),
    );

    expect(await screen.findByText('PolySaver 2.7.0')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Mettre à jour' })).toBeEnabled();
  });
});

describe('App startup with an update available', () => {
  const settings: AppSettingsDto = {
    downloadDirectory: '~/Downloads/PolySaver',
    themeMode: 'system',
    defaultPreset: { format: 'mp4', videoQuality: 'p1080' },
    language: 'fr',
  };

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
    vi.spyOn(defaultIpcClient, 'checkJsRuntime').mockResolvedValue({
      kind: 'node',
      isReady: true,
      versionTooOld: false,
    });
    vi.spyOn(defaultIpcClient, 'checkForUpdates').mockResolvedValue(update);
    vi.spyOn(defaultIpcClient, 'getReleaseNotes').mockResolvedValue({
      name: 'PolySaver 2.7.0',
      body: RELEASE_BODY,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows the green button but no popup, and opens the dialog on click', async () => {
    const user = userEvent.setup();
    render(<App />);

    const button = await screen.findByRole('button', {
      name: 'Mise à jour disponible : PolySaver 2.7.0',
    });
    // No dialog opened by itself at startup.
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await user.click(button);
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
    expect(
      await screen.findByText('PolySaver 2.7.0: Playlists done right, smarter downloads'),
    ).toBeInTheDocument();
  }, 20000);
});

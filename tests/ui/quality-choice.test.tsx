// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider } from '@mui/material';
import '../../src/i18n';
import { setAppLanguage } from '../../src/i18n';
import { createAppTheme } from '../../src/app/theme';
import {
  DownloadOptionsDialog,
  closestAvailableQuality,
} from '../../src/components/DownloadOptionsDialog';
import type { DownloadPresetDto, ProbeResult } from '../../src/ipc/contracts';

const offered = [
  { quality: 'p1080' as const, height: 1080 },
  { quality: 'p720' as const, height: 720 },
  { quality: 'p240' as const, height: 240 },
];

const singleVideo: ProbeResult = {
  url: 'https://www.youtube.com/watch?v=x3f2-4h3AsM',
  title: 'Michel Jonasz - Les fourmis rouges',
  durationSeconds: 240,
  formats: [
    { formatId: '137', extension: 'mp4', height: 1080, hasVideo: true, hasAudio: false, filesizeApproxBytes: 60_000_000 },
    { formatId: '136', extension: 'mp4', height: 720, hasVideo: true, hasAudio: false, filesizeApproxBytes: 30_000_000 },
    { formatId: '140', extension: 'm4a', hasVideo: false, hasAudio: true, filesizeApproxBytes: 4_000_000 },
  ],
  availableVideoQualities: ['best', 'p1080', 'p720'],
};

function renderDialog(defaultPreset: DownloadPresetDto, onConfirmDownload = vi.fn()) {
  render(
    <ThemeProvider theme={createAppTheme('light')}>
      <DownloadOptionsDialog
        open={true}
        onClose={vi.fn()}
        probeResult={singleVideo}
        isLoading={false}
        defaultPreset={defaultPreset}
        defaultDownloadDirectory="~/Downloads/PolySaver"
        onConfirmDownload={onConfirmDownload}
      />
    </ThemeProvider>,
  );
  return onConfirmDownload;
}

describe('closestAvailableQuality (same rule as the engine)', () => {
  it('keeps the wanted quality when offered', () => {
    expect(closestAvailableQuality('p720', offered)).toBe('p720');
  });

  it('falls back to the highest quality not above the wanted one', () => {
    expect(closestAvailableQuality('p2160', offered)).toBe('p1080');
    expect(closestAvailableQuality('p480', offered)).toBe('p240');
  });

  it('takes the lowest quality above when nothing is lower', () => {
    expect(closestAvailableQuality('p144', offered)).toBe('p240');
  });

  it('maps best to the highest offered quality', () => {
    expect(closestAvailableQuality('best', offered)).toBe('p1080');
  });
});

describe('Download dialog quality list for a single video', () => {
  beforeEach(async () => {
    await setAppLanguage('fr');
  });

  it('lists only real qualities, sized in Mo, and preselects the closest one', async () => {
    const user = userEvent.setup();
    // 4K by default, but this video tops out at 1080p.
    const onConfirm = renderDialog({ format: 'mp4', videoQuality: 'p2160' });

    // No separate "Best" entry: 1080p already is the best one.
    expect(screen.queryByText('Meilleure')).not.toBeInTheDocument();
    expect(screen.getByText('1080p · Full HD')).toBeInTheDocument();
    expect(screen.getByText('720p · HD')).toBeInTheDocument();
    // French sizes use Mo, never MB.
    expect(screen.getAllByText(/\d+ Mo$/).length).toBeGreaterThan(0);
    expect(screen.queryByText(/\d+ MB$/)).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /lancer le téléchargement/i }));
    expect(onConfirm.mock.calls[0][0]).toEqual({ format: 'mp4', videoQuality: 'p1080' });
  });
});

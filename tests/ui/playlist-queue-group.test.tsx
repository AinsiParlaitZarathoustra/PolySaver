// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { ThemeProvider } from '@mui/material';
import '../../src/i18n';
import { setAppLanguage } from '../../src/i18n';
import { createAppTheme } from '../../src/app/theme';
import { DownloadQueue, groupQueueItems } from '../../src/components/DownloadQueue';
import type { DownloadJobDto, DownloadStatus } from '../../src/ipc/contracts';

const GROUP = { id: 'group-1', title: 'Ma playlist', total: 3 };

function job(
  id: string,
  status: DownloadStatus,
  position?: number,
  progressPercent: number | null = null,
): DownloadJobDto {
  return {
    id,
    url: `https://www.youtube.com/watch?v=${id}`,
    preset: { format: 'mp4', videoQuality: 'best' },
    title: `Vidéo ${id}`,
    status,
    progressPercent,
    retryCount: 0,
    playlist: position === undefined ? undefined : { ...GROUP, position },
  };
}

function renderQueue(jobs: DownloadJobDto[], handlers = {}) {
  return render(
    <ThemeProvider theme={createAppTheme('light')}>
      <DownloadQueue jobs={jobs} {...handlers} />
    </ThemeProvider>,
  );
}

describe('Playlist group in the download queue', () => {
  beforeEach(async () => {
    await setAppLanguage('fr');
  });

  it('folds one submission into a single item, in playlist order', () => {
    const items = groupQueueItems([
      job('solo-a', 'downloading'),
      job('p3', 'queued', 3),
      job('p1', 'completed', 1),
      job('solo-b', 'completed'),
      job('p2', 'downloading', 2, 40),
    ]);

    expect(items.map((i) => (i.kind === 'job' ? i.job.id : i.id))).toEqual([
      'solo-a',
      'group-1',
      'solo-b',
    ]);
    const group = items[1];
    expect(group.kind === 'playlist' && group.jobs.map((j) => j.id)).toEqual(['p1', 'p2', 'p3']);
  });

  it('collapses and expands the videos, each with its own progress bar', async () => {
    const user = userEvent.setup();
    renderQueue([
      job('p1', 'completed', 1),
      job('p2', 'downloading', 2, 40),
      job('p3', 'queued', 3),
    ]);

    expect(screen.getByText('Ma playlist')).toBeInTheDocument();
    expect(screen.getByText('1/3 téléchargées')).toBeInTheDocument();
    // Collapsed by default: only the overall bar, no video row.
    expect(screen.queryByText('Vidéo p2')).not.toBeInTheDocument();
    expect(screen.getByTestId('KeyboardArrowRightIcon')).toBeInTheDocument();
    // (1 finished video at 100 % + 40 % + 0 %) / 3 videos
    expect(screen.getByRole('progressbar', { name: '1/3 téléchargées' })).toHaveAttribute(
      'aria-valuenow',
      '47',
    );

    await user.click(screen.getByRole('button', { name: /afficher les vidéos de la playlist/i }));

    expect(screen.getByTestId('KeyboardArrowDownIcon')).toBeInTheDocument();
    for (const title of ['Vidéo p1', 'Vidéo p2', 'Vidéo p3']) {
      expect(screen.getByText(title)).toBeInTheDocument();
    }
    // The downloading video shows its own bar next to the overall one.
    expect(screen.getAllByRole('progressbar').length).toBeGreaterThan(1);

    await user.click(screen.getByRole('button', { name: /masquer les vidéos de la playlist/i }));
    expect(screen.getByTestId('KeyboardArrowRightIcon')).toBeInTheDocument();
  });

  it('retries only the failed videos and cancels only the running ones', async () => {
    const user = userEvent.setup();
    const onRetryJob = vi.fn();
    const onCancelJob = vi.fn().mockResolvedValue(undefined);
    renderQueue(
      [
        job('p1', 'failed', 1),
        job('p2', 'downloading', 2, 10),
        job('p3', 'failed', 3),
        job('p4', 'completed', 4),
      ],
      { onRetryJob, onCancelJob },
    );

    expect(screen.getByText('1/4 téléchargées · 2 en échec')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Réessayer les échecs (2)' }));
    expect(onRetryJob.mock.calls.map(([id]) => id)).toEqual(['p1', 'p3']);

    await user.click(screen.getByRole('button', { name: 'Tout annuler' }));
    expect(onCancelJob.mock.calls.map(([id]) => id)).toEqual(['p2']);
  });

  it('shows a summary and lets the finished playlist be removed', async () => {
    const user = userEvent.setup();
    const onDismissJob = vi.fn().mockResolvedValue(undefined);
    const { container } = renderQueue(
      [job('p1', 'completed', 1), job('p2', 'failed', 2), job('p3', 'completed', 3)],
      { onDismissJob },
    );

    const card = within(container);
    expect(card.getByText('2 téléchargées, 1 en échec')).toBeInTheDocument();
    // Everything is finished: no overall bar left.
    expect(card.queryByRole('progressbar')).not.toBeInTheDocument();

    await user.click(card.getByRole('button', { name: 'Retirer la playlist de la file' }));
    expect(onDismissJob.mock.calls.map(([id]) => id)).toEqual(['p1', 'p2', 'p3']);
  });
});

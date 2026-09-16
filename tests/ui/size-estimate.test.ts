// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect } from 'vitest';
import { estimateDownloadSize, formatBytes, resolveFormatBytes } from '../../src/utils/sizeEstimate';
import type { FormatOption } from '../../src/ipc/contracts';

const format = (partial: Partial<FormatOption> & { formatId: string }): FormatOption => ({
  hasVideo: false,
  hasAudio: false,
  extension: 'mp4',
  ...partial,
});

/**
 * Realistic YouTube-like format set: low-bitrate DRM-ish audio first (the old
 * buggy implementation picked exactly this one), then video-only streams of
 * increasing quality plus a high-quality audio stream.
 */
const youtubeFormats: FormatOption[] = [
  format({ formatId: '139-drc', extension: 'm4a', hasAudio: true, filesizeApproxBytes: 3_870_000, tbr: 48 }),
  format({ formatId: '140', extension: 'm4a', hasAudio: true, filesizeApproxBytes: 5_000_000, tbr: 128 }),
  format({ formatId: '251', extension: 'webm', hasAudio: true, filesizeApproxBytes: 6_000_000, tbr: 160 }),
  format({ formatId: '160', extension: 'mp4', hasVideo: true, height: 144, filesizeApproxBytes: 2_000_000, tbr: 80 }),
  format({ formatId: '133', extension: 'mp4', hasVideo: true, height: 240, filesizeApproxBytes: 4_000_000, tbr: 160 }),
  format({ formatId: '134', extension: 'mp4', hasVideo: true, height: 360, filesizeApproxBytes: 8_000_000, tbr: 320 }),
  format({ formatId: '135', extension: 'mp4', hasVideo: true, height: 480, filesizeApproxBytes: 16_000_000, tbr: 640 }),
  format({ formatId: '136', extension: 'mp4', hasVideo: true, height: 720, filesizeApproxBytes: 40_000_000, tbr: 1_600 }),
  format({ formatId: '137', extension: 'mp4', hasVideo: true, height: 1080, filesizeApproxBytes: 120_000_000, tbr: 4_800 }),
  format({ formatId: '401', extension: 'mp4', hasVideo: true, height: 2160, filesizeApproxBytes: 540_000_000, tbr: 21_600 }),
  // Muxed fallback (video + audio in one stream)
  format({ formatId: '22', extension: 'mp4', hasVideo: true, hasAudio: true, height: 720, filesizeApproxBytes: 45_000_000 }),
];

describe('sizeEstimate', () => {
  it('estimates "best" as the largest video stream plus the best audio', () => {
    const estimate = estimateDownloadSize(youtubeFormats, 'best');
    // 540 MB (2160p video) + 5 MB (best m4a audio, preferred by the selector),
    // never the 3.87 MB low-bitrate audio track.
    expect(estimate).toBe(545_000_000);
  });

  it('estimates a specific height with video + audio combined', () => {
    const estimate = estimateDownloadSize(youtubeFormats, 1080);
    expect(estimate).toBe(120_000_000 + 5_000_000);
  });

  it('falls back to a muxed format when no separate video stream exists', () => {
    const muxedOnly = [
      format({ formatId: '18', extension: 'mp4', hasVideo: true, hasAudio: true, height: 360, filesizeApproxBytes: 9_000_000 }),
    ];
    expect(estimateDownloadSize(muxedOnly, 360)).toBe(9_000_000);
  });

  it('returns null when no size can be determined for the requested height', () => {
    expect(estimateDownloadSize(youtubeFormats, 4320)).toBeNull();
  });

  it('returns the audio size for an audio-only request', () => {
    const audioOnly = [
      format({ formatId: '140', extension: 'm4a', hasAudio: true, filesizeApproxBytes: 5_000_000 }),
    ];
    expect(estimateDownloadSize(audioOnly, 'best')).toBe(5_000_000);
  });

  it('uses duration x tbr when filesize is missing (HLS/DASH)', () => {
    const hls = [
      format({ formatId: 'hls-v', extension: 'mp4', hasVideo: true, height: 720, tbr: 2_000 }),
      format({ formatId: 'hls-a', extension: 'm4a', hasAudio: true, tbr: 128 }),
    ];
    // 100 s at 2000 kbps = 25,000,000 bytes; 100 s at 128 kbps = 1,600,000 bytes.
    expect(estimateDownloadSize(hls, 720, 100)).toBe(25_000_000 + 1_600_000);
  });

  it('returns null when a needed size is unknown and tbr/duration are absent', () => {
    const incomplete = [
      format({ formatId: 'v', extension: 'mp4', hasVideo: true, height: 720 }),
      format({ formatId: 'a', extension: 'm4a', hasAudio: true, filesizeApproxBytes: 1_000_000 }),
    ];
    expect(estimateDownloadSize(incomplete, 720)).toBeNull();
  });

  it('prefers the mp4 video stream over other containers at the same height', () => {
    const mixed = [
      format({ formatId: 'webm', extension: 'webm', hasVideo: true, height: 720, filesizeApproxBytes: 30_000_000 }),
      format({ formatId: 'mp4', extension: 'mp4', hasVideo: true, height: 720, filesizeApproxBytes: 20_000_000 }),
    ];
    // mp4 is preferred by the real selector, so its size (20 MB) is used.
    expect(estimateDownloadSize(mixed, 720)).toBe(20_000_000);
  });

  it('resolveFormatBytes prefers the explicit filesize', () => {
    expect(resolveFormatBytes({ ...format({ formatId: 'x', hasVideo: true }), filesizeApproxBytes: 42 }, 100)).toBe(42);
  });

  it('never formats a size with a tilde prefix', () => {
    expect(formatBytes(3_870_000)).toBe('4 MB');
    expect(formatBytes(540_000_000)).toBe('515 MB');
    expect(formatBytes(2_000_000_000)).toBe('1.9 GB');
    for (const bytes of [1_000, 3_870_000, 540_000_000, 2_000_000_000]) {
      expect(formatBytes(bytes)).not.toContain('~');
    }
    expect(formatBytes(0)).toBe('');
    expect(formatBytes(null)).toBe('');
  });
});

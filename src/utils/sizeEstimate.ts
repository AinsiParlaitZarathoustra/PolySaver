// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

/**
 * Size estimation mirroring PolySaver's real yt-dlp format selector:
 * `bestvideo[height=H][ext=mp4]+bestaudio[ext=m4a] / bestvideo[height=H]+bestaudio / best[height=H]`.
 *
 * The previous implementation picked "the first format with a size", which is
 * usually a low-bitrate audio track, and could return wildly wrong values.
 */

import type { FormatOption } from '../ipc/contracts';

/** Candidate quality level; `best` means "no height constraint". */
export type SizeEstimateQuality = 'best' | number;

interface SizedFormat {
  format: FormatOption;
  size: number | null;
  height: number;
  tbr: number;
}

/**
 * Resolves a format size in bytes, falling back to `duration × tbr` when the
 * provider did not expose `filesize_approx_bytes` (typical for HLS/DASH).
 */
export function resolveFormatBytes(
  format: FormatOption,
  durationSeconds?: number | null,
): number | null {
  if (format.filesizeApproxBytes && format.filesizeApproxBytes > 0) {
    return format.filesizeApproxBytes;
  }
  const tbr = format.tbr ?? null;
  if (tbr && tbr > 0 && durationSeconds && durationSeconds > 0) {
    // tbr is in kbps; convert to bytes.
    return Math.round((durationSeconds * tbr * 1000) / 8);
  }
  return null;
}

function toSized(format: FormatOption, durationSeconds?: number | null): SizedFormat {
  return {
    format,
    size: resolveFormatBytes(format, durationSeconds),
    height: format.height ?? 0,
    tbr: format.tbr ?? 0,
  };
}

/** Comparator implementing yt-dlp's simplified preference order: height, then size, then tbr. */
function compareBest(a: SizedFormat, b: SizedFormat): number {
  if (a.height !== b.height) return b.height - a.height;
  const aSize = a.size ?? 0;
  const bSize = b.size ?? 0;
  if (aSize !== bSize) return bSize - aSize;
  return b.tbr - a.tbr;
}

/**
 * Estimates the downloaded size (video + audio) for a requested quality.
 *
 * Returns `null` when the size cannot be determined reliably; callers must then
 * display no value rather than an arbitrary number.
 */
export function estimateDownloadSize(
  formats: FormatOption[],
  quality: SizeEstimateQuality,
  durationSeconds?: number | null,
): number | null {
  if (!formats || formats.length === 0) return null;

  const targetHeight = quality === 'best' ? null : quality;

  const withinHeight = (f: FormatOption): boolean =>
    targetHeight === null || f.height === targetHeight;

  const videoOnly = formats
    .filter((f) => f.hasVideo && !f.hasAudio && withinHeight(f))
    .map((f) => toSized(f, durationSeconds));

  // Prefer mp4 video streams, as the real selector does.
  const mp4Video = videoOnly.filter((v) => v.format.extension.toLowerCase() === 'mp4');
  const video = (mp4Video.length > 0 ? mp4Video : videoOnly).sort(compareBest)[0] ?? null;

  const audioOnly = formats
    .filter((f) => f.hasAudio && !f.hasVideo)
    .map((f) => toSized(f, durationSeconds));

  const m4aAudio = audioOnly.filter((a) => a.format.extension.toLowerCase() === 'm4a');
  const audio = (m4aAudio.length > 0 ? m4aAudio : audioOnly).sort(compareBest)[0] ?? null;

  if (video) {
    // A separate video stream is always muxed with the best audio by the selector.
    if (!audio) return video.size;
    if (video.size === null || audio.size === null) return null;
    return video.size + audio.size;
  }

  // No separate video stream: fall back to a muxed format (has both video and audio).
  const muxed = formats
    .filter((f) => f.hasVideo && f.hasAudio && withinHeight(f))
    .map((f) => toSized(f, durationSeconds))
    .sort(compareBest)[0];

  if (muxed) {
    return muxed.size;
  }

  // A specific video height was requested but nothing matches it: no estimate
  // at all, rather than a misleading audio-only number.
  if (targetHeight !== null) {
    return null;
  }

  // 'best' with no video at all: audio-only request (e.g. MP3/FLAC preset).
  if (audio) return audio.size;
  return null;
}

/**
 * Formats a byte count for display. Never prefixes with `~`: the UI shows the
 * estimate value alone.
 */
export function formatBytes(bytes?: number | null): string {
  if (!bytes || bytes <= 0) return '';
  const mib = bytes / (1024 * 1024);
  if (mib >= 1024) {
    return `${(mib / 1024).toFixed(1)} GB`;
  }
  return `${Math.round(mib)} MB`;
}

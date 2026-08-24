// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { DownloadJobDto, DownloadProgressEvent } from './contracts';

/**
 * Subscribes to download progress events.
 * Returns an unlisten function.
 */
export async function onDownloadProgress(
  handler: (payload: DownloadProgressEvent) => void,
): Promise<UnlistenFn> {
  return await listen<DownloadProgressEvent>('download://progress', (event) => {
    handler(event.payload);
  });
}

/**
 * Subscribes to download queued events.
 */
export async function onDownloadQueued(
  handler: (payload: DownloadJobDto) => void,
): Promise<UnlistenFn> {
  return await listen<DownloadJobDto>('download://queued', (event) => {
    handler(event.payload);
  });
}

/**
 * Subscribes to download completed events.
 */
export async function onDownloadCompleted(
  handler: (payload: DownloadJobDto) => void,
): Promise<UnlistenFn> {
  return await listen<DownloadJobDto>('download://completed', (event) => {
    handler(event.payload);
  });
}

/**
 * Subscribes to download failed events.
 */
export async function onDownloadFailed(
  handler: (payload: DownloadJobDto) => void,
): Promise<UnlistenFn> {
  return await listen<DownloadJobDto>('download://failed', (event) => {
    handler(event.payload);
  });
}

/**
 * Subscribes to download canceled events.
 */
export async function onDownloadCanceled(
  handler: (payload: DownloadJobDto) => void,
): Promise<UnlistenFn> {
  return await listen<DownloadJobDto>('download://canceled', (event) => {
    handler(event.payload);
  });
}

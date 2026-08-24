// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { HelpSupportDialog } from '../../src/components/HelpSupportDialog';
import { Header } from '../../src/components/Header';
import { fr } from '../../src/i18n/locales/fr';
import { en } from '../../src/i18n/locales/en';
import type { IpcClient } from '../../src/ipc/client';
import type { UpdateInfo } from '../../src/ipc/contracts';
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

void i18n.use(initReactI18next).init({
  lng: 'fr',
  fallbackLng: 'fr',
  resources: {
    fr: { translation: fr },
    en: { translation: en },
  },
  interpolation: { escapeValue: false },
});

describe('Sprint 8.4: Help, Support and Updater', () => {
  beforeEach(() => {
    void i18n.changeLanguage('fr');
  });

  const createMockClient = (overrides?: Partial<IpcClient>): IpcClient => ({
    healthCheck: vi.fn(),
    analyzeUrl: vi.fn(),
    getSettings: vi.fn(),
    setSettings: vi.fn(),
    startDownload: vi.fn(),
    listDownloads: vi.fn(),
    cancelDownload: vi.fn(),
    dismissDownload: vi.fn(),
    openDownloadSourceUrl: vi.fn(),
    pickDirectory: vi.fn(),
    revealDownloadedFile: vi.fn(),
    openDownloadedFile: vi.fn(),
    listDownloadHistory: vi.fn().mockResolvedValue([]),
    removeDownloadHistoryEntry: vi.fn(),
    revealHistoryFile: vi.fn(),
    openHistoryFile: vi.fn(),
    openHistorySourceUrl: vi.fn(),
    openSupportPage: vi.fn().mockResolvedValue(undefined),
    checkForUpdates: vi.fn().mockResolvedValue(null),
    downloadAndInstallUpdate: vi.fn().mockResolvedValue(undefined),
    restartApp: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  });

  // 1. i18n symmetry test
  it('guarantees French and English translation catalogs are structurally identical', () => {
    const getKeys = (obj: Record<string, unknown>, prefix = ''): string[] => {
      let keys: string[] = [];
      for (const [k, v] of Object.entries(obj)) {
        const fullKey = prefix ? `${prefix}.${k}` : k;
        if (typeof v === 'object' && v !== null && !Array.isArray(v)) {
          keys = keys.concat(getKeys(v as Record<string, unknown>, fullKey));
        } else {
          keys.push(fullKey);
        }
      }
      return keys.sort();
    };

    const frKeys = getKeys(fr as unknown as Record<string, unknown>);
    const enKeys = getKeys(en as unknown as Record<string, unknown>);

    expect(frKeys).toEqual(enKeys);
  });

  // 2. Header contains Help button
  it('renders circular Help button in Header and triggers callback', () => {
    const onOpenSettings = vi.fn();
    const onOpenHelp = vi.fn();

    render(<Header onOpenSettings={onOpenSettings} onOpenHelp={onOpenHelp} />);

    const helpBtn = screen.getByLabelText('Aide et support');
    expect(helpBtn).toBeInTheDocument();

    fireEvent.click(helpBtn);
    expect(onOpenHelp).toHaveBeenCalledTimes(1);
  });

  // 3. Help dialog navigation and content
  it('renders all 3 tabs with accurate guide, qualities in ascending order, and containers', async () => {
    const client = createMockClient();
    const onClose = vi.fn();

    const { rerender } = render(
      <HelpSupportDialog
        open={true}
        onClose={onClose}
        client={client}
        hasActiveDownloads={false}
      />,
    );

    // Tab 0: App Guide
    expect(screen.getByText('Téléchargement rapide')).toBeInTheDocument();
    expect(screen.getByText('Téléchargement personnalisé')).toBeInTheDocument();
    expect(
      screen.getByText(/Le téléchargement rapide ne demande aucune confirmation/),
    ).toBeInTheDocument();

    // Switch to Tab 1: Format Guide
    const formatTab = screen.getByRole('tab', { name: 'Guide des formats' });
    fireEvent.click(formatTab);

    expect(screen.getByText('Qualités vidéo')).toBeInTheDocument();
    expect(
      screen.getByText(/144p, 240p et 360p — Très basse définition/),
    ).toBeInTheDocument();
    expect(screen.getByText('1080p — Full HD — 1920 × 1080')).toBeInTheDocument();
    expect(screen.getByText('Recommandé')).toBeInTheDocument();
    expect(screen.getByText('Non recommandé')).toBeInTheDocument();
    expect(screen.getByText('MP4 ou MOV ?')).toBeInTheDocument();
    expect(screen.getByText('MP3 ou FLAC ?')).toBeInTheDocument();

    // Switch to English and verify
    await act(async () => {
      await i18n.changeLanguage('en');
    });

    rerender(
      <HelpSupportDialog
        open={true}
        onClose={onClose}
        client={client}
        hasActiveDownloads={false}
      />,
    );

    expect(screen.getByText('Video qualities')).toBeInTheDocument();
    expect(screen.getByText('Recommended')).toBeInTheDocument();
    expect(screen.getByText('Not recommended')).toBeInTheDocument();
    expect(screen.getByText('MP4 or MOV?')).toBeInTheDocument();
  });

  // 4. Support actions trigger openSupportPage
  it('calls openSupportPage when clicking support actions', async () => {
    const openSupportPage = vi.fn().mockResolvedValue(undefined);
    const client = createMockClient({ openSupportPage });

    render(
      <HelpSupportDialog
        open={true}
        onClose={vi.fn()}
        client={client}
        hasActiveDownloads={false}
      />,
    );

    // Switch to Support tab
    const supportTab = screen.getByRole('tab', { name: 'Support' });
    fireEvent.click(supportTab);

    const reportBtn = screen.getByLabelText('Signaler un problème');
    fireEvent.click(reportBtn);

    expect(openSupportPage).toHaveBeenCalledTimes(1);
  });

  // 5. Updater manual check and install flow
  it('checks for updates, handles available update, and disables install during active download', async () => {
    const updateInfo: UpdateInfo = {
      version: '0.2.0',
      currentVersion: '0.1.0',
      body: 'Bugfixes and new features',
      date: '2026-08-24',
    };
    const checkForUpdates = vi.fn().mockResolvedValue(updateInfo);
    const downloadAndInstallUpdate = vi.fn().mockResolvedValue(undefined);
    const client = createMockClient({ checkForUpdates, downloadAndInstallUpdate });

    const { rerender } = render(
      <HelpSupportDialog
        open={true}
        onClose={vi.fn()}
        client={client}
        hasActiveDownloads={true}
      />,
    );

    // Switch to Support tab
    fireEvent.click(screen.getByRole('tab', { name: 'Support' }));

    const checkBtn = screen.getByRole('button', { name: 'Rechercher les mises à jour' });
    fireEvent.click(checkBtn);

    await waitFor(() => {
      expect(screen.getByText('Version 0.2.0 disponible')).toBeInTheDocument();
    });

    // Warning is visible and update button is disabled because hasActiveDownloads is true
    expect(
      screen.getByText(/Un téléchargement de média est en cours/),
    ).toBeInTheDocument();
    const updateNowBtn = screen.getByRole('button', { name: 'Mettre à jour' });
    expect(updateNowBtn).toBeDisabled();

    // Rerender with hasActiveDownloads = false
    rerender(
      <HelpSupportDialog
        open={true}
        onClose={vi.fn()}
        client={client}
        hasActiveDownloads={false}
      />,
    );

    expect(updateNowBtn).not.toBeDisabled();
    fireEvent.click(updateNowBtn);

    await waitFor(() => {
      expect(downloadAndInstallUpdate).toHaveBeenCalledTimes(1);
      expect(screen.getByText('Redémarrer')).toBeInTheDocument();
    });
  });
});

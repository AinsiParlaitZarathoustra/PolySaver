<div align="center">

# PolySaver

**Pour le refus de payer ce qui devrait être gratuit**

Téléchargeur vidéo et audio **rapide 🚀** pour macOS sans publicité ni abonnement.

[![Version](https://img.shields.io/badge/version-1.5-3b82f6)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)
[![Platform](https://img.shields.io/badge/macOS-14%2B-000000?logo=apple)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)

[**⬇ Télécharger PolySaver V1.5**([https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/PolySaver_V1.5_ARM64/PolySaver_1.5.0_aarch64.dmg](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_Windows)

</div>

---

## Pourquoi PolySaver

Les téléchargeurs vidéo du marché veulent vous vendre un abonnement pour des fonctions de base, affichent de la publicité, ou vous imposent des limites de téléchargement pour vous forcer à payer 💰. PolySaver fait la même chose en (beaucoup) mieux : une app  de quelques mégaoctets, écrite en Rust ⚡️🔒, qui ne demande rien et ne renvoie rien et vous permet de télécharger sur votre ordinateur sans limite aucune les contenus de votre choix.

## Fonctionnalités

### Deux façons de télécharger

**⚡ Téléchargement rapide** — Un clic. L'URL est analysée et le fichier téléchargé avec vos réglages par défaut. 

**Téléchargement personnalisé** — Un panneau s'ouvre avec la miniature de la vidéo : choisissez le format, la qualité, le débit audio et la langue des sous-titres avant de lancer.

### Formats

| Vidéo | Audio |
|---|---|
| MP4, MOV | MP3, FLAC, WAV, AAC |
| Sélection de la résolution | Débit configurable (128 / 192 / 320 kbps) |

### Sources

YouTube, Vimeo, X, TikTok, Instagram, Twitch, Dailymotion, Arte, France TV, Bandcamp — et plusieurs centaines d'autres plateformes.

### Le reste

- **Sous-titres** — récupérés automatiquement dans votre langue quand ils existent
- **Plusieurs téléchargements à la fois** — activable en un clic dans les réglages
- **Vitesse ajustable** — plafonnez le débit pour garder de la bande passante pour le reste
- **Bilingue** — français et anglais

## Installation

1. Téléchargez le fichier `.dmg` depuis la page [**Releases**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)
2. Ouvrez-le et glissez PolySaver dans le dossier Applications
3. Au premier lancement : clic droit sur l'app → **Ouvrir** (l'application n'est pas signée par Apple)

> **Configuration requise** — MacBook M1 ou ultérieur.

## Raccourcis clavier

Les raccourcis disponibles sont indiqués directement dans la barre de menus de l'application.

## Architecture

| Couche | Technologie |
|---|---|
| Backend | Rust — sécurité mémoire, performances réseau natives |
| Interface | Svelte — réactive, modulaire, sans surcouche |
| Extraction | [yt-dlp](https://github.com/yt-dlp/yt-dlp) |
| Conversion | [FFmpeg](https://ffmpeg.org) |
| Stockage local | [redb](https://github.com/cberner/redb) |

## Licence

PolySaver est un logiciel propriétaire, gratuit pour un usage personnel et qui ne sera **jamais** payant, mais la revente et la redistribution commerciale sont interdites. Voir [LICENSE](https://github.com/AinsiParlaitZarathoustra/PolySaver?tab=License-1-ov-file).

## Avertissement

PolySaver est un outil technique. Respectez le droit d'auteur et **les conditions d'utilisation des plateformes dont vous téléchargez du contenu**.

---

<div align="center">

Copyright © 2026 AinsiParlaitZarathoustra — Tous droits réservés

*Amen*

</div>

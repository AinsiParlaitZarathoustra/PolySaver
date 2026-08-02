<div align="center">

# PolySaver

**Pour le refus de payer ce qui devrait être gratuit**

Téléchargeur vidéo et audio natif sans publicité ni abonnement et **rapide 🚀**.

[![macOS](https://img.shields.io/badge/macOS-14%2B-000000?logo=apple&logoColor=white)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/PolySaver_V1.5_ARM64/PolySaver_1.5.0_aarch64.dmg)
[![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6?logo=windows&logoColor=white)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_Windows)
[![Reddit](https://img.shields.io/badge/Reddit-r%2FPolySaver-FF4500?logo=reddit&logoColor=white)](https://www.reddit.com/r/PolySaver/)

[**⬇ Télécharger pour macOS (ARM64)**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_ARM64) · [**⬇ Télécharger pour Windows**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_Windows) · [**💬 Communauté Reddit**](https://www.reddit.com/r/PolySaver/)

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

### 🍎 macOS

1. Téléchargez le fichier `.dmg` depuis la page [**Releases**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_ARM64)
2. Ouvrez-le et glissez PolySaver dans le dossier Applications
3. Au premier lancement, macOS affichera un avertissement car l'application n'est pas signée par un compte développeur Apple (ce qui coûte une centaine d'euros par an, hors de propos pour un outil gratuit) :
   - Clic droit sur l'app → **Ouvrir**
   - Confirmez **Ouvrir** dans la boîte de dialogue

> **Configuration requise** — macOS 14 Sonoma ou ultérieur, Apple Silicon M1 ou ultérieur.

### 🪟 Windows

1. Téléchargez le fichier `setup.exe` depuis la page [**Releases**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_Windows)
2. Lancez l'installeur. Windows SmartScreen peut afficher un avertissement car l'application n'est pas signée par un certificat reconnu :
   - Cliquez sur **Informations complémentaires**
   - Cliquez sur **Exécuter quand même**
3. Suivez les étapes de l'installeur (Suivant → Installer → Terminer)

> **Configuration requise** — Windows 10 (22H2) ou Windows 11.

## Laisser un avis

- **Possibilité de nous contacter sur Reddit** → [r/PolySaver](https://www.reddit.com/r/PolySaver/)
- **Possibilité de déposer une "issue"** si vous rencontrez un bug ou souhaitez proposer quelque chose

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

</div>2. Ouvrez-le et glissez PolySaver dans le dossier Applications
3. Au premier lancement, macOS affichera un avertissement car l'application n'est pas signée par un compte développeur Apple (ce qui coûte une centaine d'euros par an, hors de propos pour un outil gratuit) :
   - Clic droit sur l'app → **Ouvrir**
   - Confirmez **Ouvrir** dans la boîte de dialogue

> **Configuration requise** — macOS 14 Sonoma ou ultérieur, Apple Silicon M1 ou ultérieur.

### 🪟 Windows

1. Téléchargez le fichier `setup.exe` depuis la page [**Releases**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases#release-PolySaver_V1.5_Windows)
2. Lancez l'installeur. Windows SmartScreen peut afficher un avertissement car l'application n'est pas signée par un certificat reconnu :
   - Cliquez sur **Informations complémentaires**
   - Cliquez sur **Exécuter quand même**
3. Suivez les étapes de l'installeur (Suivant → Installer → Terminer)

> **Configuration requise** — Windows 10 (22H2) ou Windows 11.

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

<div align="center">

# PolySaver

**Collez. Téléchargez. Profitez.**

Téléchargeur vidéo et audio natif pour macOS. Rapide, gratuit, sans compte, sans publicité, sans abonnement.

[![Version](https://img.shields.io/badge/version-1.5-3b82f6)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)
[![Platform](https://img.shields.io/badge/macOS-14%2B-000000?logo=apple)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-Tauri%20v2-CE422B?logo=rust)](https://tauri.app)

[**⬇ Télécharger PolySaver V1.5**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)

</div>

---

## Pourquoi PolySaver

Les téléchargeurs vidéo du marché facturent un abonnement pour des fonctions basiques, affichent de la publicité, ou pèsent 200 Mo pour un simple champ de saisie. PolySaver fait la même chose en mieux : un binaire natif de quelques mégaoctets, écrit en Rust, qui ne demande rien et ne renvoie rien.

## Fonctionnalités

### Deux façons de télécharger

**⚡ Téléchargement rapide** — Un clic. L'URL est analysée et le fichier téléchargé avec vos réglages par défaut. Aucune question posée.

**Téléchargement personnalisé** — Un panneau s'ouvre avec la miniature de la vidéo : choisissez le format, la qualité, le débit audio et la langue des sous-titres avant de lancer.

### Formats

| Vidéo | Audio |
|---|---|
| MP4, MOV | MP3, FLAC, WAV, AAC |
| Sélection de la résolution | Débit configurable (128 / 192 / 320 kbps) |

### Sources

YouTube, Vimeo, X, TikTok, Instagram, Twitch, Dailymotion, Arte, France TV, Bandcamp — et plusieurs centaines d'autres plateformes.

### Le reste

- **Playlists** — sélectionnez les vidéos une par une ou téléchargez tout
- **Chapitres YouTube** — extrayez un segment précis d'une vidéo
- **Sous-titres** — récupérés automatiquement dans votre langue quand ils existent
- **Téléchargement parallèle** — activez-le et lancez autant de tâches que vous voulez
- **Limite de bande passante** — en Ko/s ou Mo/s, pour ne pas saturer votre connexion
- **Notifications macOS** — natives, avec son, à la fin de chaque téléchargement
- **Barre de menus** — icône SF Symbols qui s'adapte aux thèmes clair et sombre
- **Historique** — persistant, exportable en CSV, avec détection des doublons
- **Bilingue** — français et anglais
- **Messages d'erreur clairs** — « Absence de connexion internet » plutôt qu'une trace technique de trente lignes

## Installation

1. Téléchargez le fichier `.dmg` depuis la page [**Releases**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/latest)
2. Ouvrez-le et glissez PolySaver dans le dossier Applications
3. Au premier lancement : clic droit sur l'app → **Ouvrir** (l'application n'est pas signée par Apple)

Aucune dépendance à installer. yt-dlp et FFmpeg sont embarqués dans l'application.

> **Configuration requise** — macOS 14 (Sonoma) ou supérieur, Apple Silicon.

## Raccourcis clavier

| Raccourci | Action |
|---|---|
| `⌘V` | Coller une URL et l'analyser |
| `⌘,` | Ouvrir les préférences |
| `⌘⌫` | Vider les téléchargements terminés |
| `Escape` | Fermer le panneau ouvert |

## Architecture

| Couche | Technologie |
|---|---|
| Application | [Tauri v2](https://tauri.app) — binaire natif, pas d'Electron |
| Backend | Rust — sécurité mémoire, performances réseau natives |
| Interface | Svelte 5 + Vite — réactive, modulaire, sans surcouche |
| Extraction | [yt-dlp](https://github.com/yt-dlp/yt-dlp) — embarqué |
| Conversion | [FFmpeg](https://ffmpeg.org) — embarqué |
| Stockage local | [redb](https://github.com/cberner/redb) — base clé-valeur ACID |

## Développement

```bash
git clone https://github.com/AinsiParlaitZarathoustra/PolySaver.git
cd PolySaver
npm install
cargo tauri dev
```

Build de production :

```bash
bash scripts/download-binaries.sh   # Récupère yt-dlp et FFmpeg
cargo tauri build                    # Génère le .dmg
```

## Licence

PolySaver est un logiciel propriétaire, gratuit pour un usage personnel. Le code source est ouvert à la lecture et aux contributions, mais la revente et la redistribution commerciale sont interdites. Voir [LICENSE](LICENSE).

## Avertissement

PolySaver est un outil technique. Respectez le droit d'auteur et les conditions d'utilisation des plateformes dont vous téléchargez du contenu.

---

<div align="center">

Copyright © 2026 AinsiParlaitZarathoustra — Tous droits réservés

*Propulsé par Rust et le refus de payer pour ce qui devrait être gratuit.*

</div>

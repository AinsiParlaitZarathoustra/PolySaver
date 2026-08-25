<div align="center">

![PolySaver on PC, Mac and Linux](./banner_polysaver.png)

# PolySaver

**Pour le refus de payer ce qui devrait être gratuit**

Téléchargeur vidéo et audio sans publicité ni abonnement et **rapide 🚀**

[![macOS](https://img.shields.io/badge/macOS-14%2B-000000?logo=apple&logoColor=white)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_macOS_arm64.dmg)
[![Windows](https://img.shields.io/badge/Windows-10%2F11-0078D6?logo=windows&logoColor=white)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_Windows_x64_Setup.exe)
[![Linux](https://img.shields.io/badge/Linux-x86__64-FCC624?logo=linux&logoColor=black)](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_Linux_x64.AppImage)
[![Reddit](https://img.shields.io/badge/Reddit-r%2FPolySaver-FF4500?logo=reddit&logoColor=white)](https://www.reddit.com/r/PolySaver/)

[**⬇ Télécharger pour macOS (ARM64)**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_macOS_arm64.dmg) · [**⬇ Télécharger pour Windows**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_Windows_x64_Setup.exe) · [**⬇ Télécharger pour Linux**](https://github.com/AinsiParlaitZarathoustra/PolySaver/releases/download/v2.0.0/PolySaver_2.0.0_Linux_x64.AppImage) · [**💬 Communauté Reddit**](https://www.reddit.com/r/PolySaver/)

</div>

---

## 🆕 Version 2.0.0

La v2.0 n'est pas une simple mise à jour : c'est une refonte complète, repartie (presque) de zéro avec l'expérience acquise sur la v1.

- **Nouvelle interface** : passage de Svelte 5 à **React 19**, pour une expérience beaucoup plus confortable visuellement
- **Réécriture intégrale** : Nouvelle interface, nouvelle esthétique, optimisationS et nombreuses améliorations
- **Architecture en étoile** : nouvelle organisation technique du projet
- **Fiabilité renforcée** : correction de bugs dans la gestion de YT-DLP, pour un téléchargement de vidéos redoutablement fiable
- **Code source publié intégralement** : distribution GitHub améliorée
- **Formats simplifiés** : suppression du support des formats `.WAV` et `.AAC`

Ce qui ne change pas : le langage (**Rust**), le format et la structure du projet mais surtout l'ADN de PolySaver — sa **gratuité**. Des benchmarks de performance seront prochainement publiés.

---

## Pourquoi PolySaver

Les téléchargeurs vidéo du marché veulent vous vendre un abonnement pour des fonctions de base, affichent de la publicité, ou vous imposent des limites de téléchargement pour vous forcer à payer 💰. PolySaver fait la même chose en (beaucoup) mieux : une app de quelques mégaoctets, écrite en Rust  ⚡️🔒, qui ne demande rien, ne renvoie rien et vous permet de télécharger sur votre ordinateur sans limite aucune les contenus de votre choix sur PC, Mac et Linux

## Fonctionnalités

### Deux façons de télécharger

**⚡ Téléchargement rapide** : Un clic. PolySaver analyse l'URL et le fichier se télécharge avec vos réglages par défaut choisis dans les paramètres

**Téléchargement personnalisé** : Un panneau s'ouvre avec la miniature de la vidéo : choisissez le format, la qualité, le débit audio et le dossier de téléchargement avant de le lancer

### Formats

| Vidéo | Audio |
|---|---|
| MP4 | MP3, FLAC |
| Sélection de la résolution | Débit configurable (128 / 192 / 320 kbps) |

### Sources

YouTube, Vimeo, X, TikTok, Instagram, Twitch, Dailymotion, Arte, France TV, Bandcamp et plusieurs centaines d'autres plateformes. YT-DLP permet de télécharger depuis plus de 1700 sites... rien que ça...

### Le reste

- **Sous-titres** : récupérés automatiquement dans votre langue quand ils existent - fonctionnalité absente de la V2.0.0 de PolySaver mais prochainement rétablie
- **Plusieurs téléchargements à la fois** : pour aller plus vite
- **Multiplateforme** : disponible sur Mac, Windows et maintenant Linux

## Licence

PolySaver est, bien malgré moi, distribué sous licence **GPL 3.0**. Voir le fichier [LICENSE](./LICENSE) pour le texte complet.

Je ferai de yt-dlp un module non intégré à mon logiciel dans les prochaines versions, pour pouvoir passer PolySaver en Apache 2.0 ou similaire...

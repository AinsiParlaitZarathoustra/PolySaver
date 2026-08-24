// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';

const [releaseDirectoryArg, version, tag, repository] = process.argv.slice(2);

if (!releaseDirectoryArg || !version || !tag || !repository) {
  throw new Error(
    'Usage: node scripts/generate-release-metadata.mjs <release-dir> <version> <tag> <owner/repository>',
  );
}

if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error('The release version is not valid SemVer: ' + version);
}

if (tag !== 'v' + version) {
  throw new Error('The release tag must be v' + version + ', received ' + tag);
}

if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
  throw new Error('The GitHub repository must use owner/name syntax');
}

const releaseDirectory = resolve(releaseDirectoryArg);
const expectedFiles = {
  macArmDmg: 'PolySaver_' + version + '_macOS_arm64.dmg',
  macArmUpdater: 'PolySaver_' + version + '_macOS_arm64.app.tar.gz',
  macArmSignature: 'PolySaver_' + version + '_macOS_arm64.app.tar.gz.sig',
  windowsExe: 'PolySaver_' + version + '_Windows_x64_Setup.exe',
  windowsExeSignature: 'PolySaver_' + version + '_Windows_x64_Setup.exe.sig',
  windowsMsi: 'PolySaver_' + version + '_Windows_x64.msi',
  windowsMsiSignature: 'PolySaver_' + version + '_Windows_x64.msi.sig',
  linuxAppImage: 'PolySaver_' + version + '_Linux_x64.AppImage',
  linuxAppImageSignature: 'PolySaver_' + version + '_Linux_x64.AppImage.sig',
  linuxDeb: 'PolySaver_' + version + '_Linux_x64.deb',
};

for (const fileName of Object.values(expectedFiles)) {
  const filePath = join(releaseDirectory, fileName);
  if (!existsSync(filePath) || !statSync(filePath).isFile() || statSync(filePath).size === 0) {
    throw new Error('Required release asset is missing or empty: ' + fileName);
  }
}

const readSignature = (fileName) => {
  const signature = readFileSync(join(releaseDirectory, fileName), 'utf8').trim();
  if (!signature) {
    throw new Error('Updater signature is empty: ' + fileName);
  }
  return signature;
};

const baseUrl =
  'https://github.com/' + repository + '/releases/download/' + encodeURIComponent(tag);
const assetUrl = (fileName) => baseUrl + '/' + encodeURIComponent(fileName);

const manifest = {
  version,
  notes: 'PolySaver ' + version,
  pub_date: new Date().toISOString(),
  platforms: {
    'darwin-aarch64': {
      signature: readSignature(expectedFiles.macArmSignature),
      url: assetUrl(expectedFiles.macArmUpdater),
    },
    'windows-x86_64': {
      signature: readSignature(expectedFiles.windowsExeSignature),
      url: assetUrl(expectedFiles.windowsExe),
    },
    'linux-x86_64': {
      signature: readSignature(expectedFiles.linuxAppImageSignature),
      url: assetUrl(expectedFiles.linuxAppImage),
    },
  },
};

writeFileSync(join(releaseDirectory, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');

const assetNames = readdirSync(releaseDirectory)
  .filter((fileName) => fileName !== 'SHA256SUMS.txt')
  .sort((left, right) => left.localeCompare(right));

const checksumLines = assetNames.map((fileName) => {
  const filePath = join(releaseDirectory, fileName);
  if (!statSync(filePath).isFile()) {
    throw new Error('Unexpected directory in release staging: ' + fileName);
  }
  const digest = createHash('sha256').update(readFileSync(filePath)).digest('hex');
  return digest + '  ' + basename(fileName);
});

writeFileSync(join(releaseDirectory, 'SHA256SUMS.txt'), checksumLines.join('\n') + '\n');

const finalAssetNames = readdirSync(releaseDirectory).sort((left, right) =>
  left.localeCompare(right),
);

if (finalAssetNames.length !== 12) {
  throw new Error(
    'Expected exactly 12 release assets, found ' +
      finalAssetNames.length +
      ': ' +
      finalAssetNames.join(', '),
  );
}

console.log('Validated five installers and generated latest.json plus SHA256SUMS.txt.');
console.log(finalAssetNames.join('\n'));

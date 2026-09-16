// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';

/**
 * Generates the updater manifest (`latest.json`) and the checksum list
 * (`SHA256SUMS.txt`) for a staged release, and validates the staged assets.
 *
 * IMPORTANT: the Tauri v2 updater validates the *entire* latest.json before it
 * compares versions. A single malformed platform entry therefore breaks updates
 * for every platform, which is why everything below is checked before writing.
 *
 * Usage:
 *   node scripts/generate-release-metadata.mjs <release-dir> <version> <tag> <owner/repository>
 *   node scripts/generate-release-metadata.mjs --self-test
 */

const SELF_TEST_FLAG = '--self-test';

/** Minimum length of a minisign signature payload (decoded), in bytes. */
const MINISIGN_MIN_PAYLOAD_LENGTH = 64;

/**
 * Validates that a `.sig` file contains a structurally valid minisign signature.
 *
 * Tauri signs artifacts with minisign and writes `<artifact>.sig`. The format is
 * two lines: an `untrusted comment: ...` line and a base64 payload that decodes
 * to a signature whose first two bytes are a signature algorithm ("Ed") followed
 * by a key id. Checking this catches a truncated or placeholder signature before
 * it is published — which would silently break the updater for every user.
 */
function validateMinisignSignature(signatureText, fileName) {
  const lines = signatureText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  if (lines.length < 2) {
    throw new Error(
      'Signature file ' + fileName + ' does not contain a minisign comment and payload',
    );
  }

  const comment = lines[0];
  const payload = lines[1];

  if (!comment.startsWith('untrusted comment:')) {
    throw new Error(
      'Signature file ' + fileName + ' does not start with a minisign "untrusted comment:" line',
    );
  }

  let decoded;
  try {
    decoded = Buffer.from(payload, 'base64');
  } catch {
    throw new Error('Signature payload in ' + fileName + ' is not valid base64');
  }

  // Re-encoding must round-trip: Buffer.from is lenient and silently drops
  // invalid characters, which would let a corrupted payload through.
  const reencoded = decoded.toString('base64').replace(/=+$/, '');
  if (reencoded !== payload.replace(/=+$/, '')) {
    throw new Error('Signature payload in ' + fileName + ' is not valid base64');
  }

  if (decoded.length < MINISIGN_MIN_PAYLOAD_LENGTH) {
    throw new Error(
      'Signature payload in ' + fileName + ' is too short (' + decoded.length + ' bytes)',
    );
  }

  // minisign signatures start with the algorithm identifier "Ed".
  if (decoded[0] !== 0x45 || decoded[1] !== 0x64) {
    throw new Error('Signature file ' + fileName + ' does not contain an minisign signature');
  }
}

/** Runs the full generation + validation against a directory of real assets. */
function generateReleaseMetadata(releaseDirectoryArg, version, tag, repository) {
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
    // Structural validation: a truncated or placeholder signature must never be
    // published, because the updater rejects the whole manifest when it is bad.
    validateMinisignSignature(signature, fileName);
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
}

/** Builds a syntactically valid minisign signature for the fixtures. */
function fakeMinisignSignature(label) {
  // "Ed" prefix followed by a key id and signature bytes, as minisign emits.
  const payload = Buffer.concat([
    Buffer.from('Ed', 'ascii'),
    Buffer.alloc(MINISIGN_MIN_PAYLOAD_LENGTH, 0x42),
  ]);
  return 'untrusted comment: ' + label + '\n' + payload.toString('base64') + '\n';
}

/**
 * Exercises the generator against synthetic fixtures so this critical script can
 * be validated in CI without building the whole application.
 */
function selfTest() {
  const version = '9.9.9';
  const tag = 'v' + version;
  const repository = 'example/example';
  const directory = mkdtempSync(join(tmpdir(), 'polysaver-metadata-'));

  try {
    const writeAsset = (suffix, content) => {
      writeFileSync(join(directory, 'PolySaver_' + version + '_' + suffix), content);
    };

    writeAsset('macOS_arm64.dmg', 'dmg-bytes');
    writeAsset('macOS_arm64.app.tar.gz', 'mac-updater-bytes');
    writeAsset('macOS_arm64.app.tar.gz.sig', fakeMinisignSignature('macOS updater'));
    writeAsset('Windows_x64_Setup.exe', 'exe-bytes');
    writeAsset('Windows_x64_Setup.exe.sig', fakeMinisignSignature('windows exe'));
    writeAsset('Windows_x64.msi', 'msi-bytes');
    writeAsset('Windows_x64.msi.sig', fakeMinisignSignature('windows msi'));
    writeAsset('Linux_x64.AppImage', 'appimage-bytes');
    writeAsset('Linux_x64.AppImage.sig', fakeMinisignSignature('linux appimage'));
    writeAsset('Linux_x64.deb', 'deb-bytes');

    generateReleaseMetadata(directory, version, tag, repository);

    // 1. Exactly the 12 expected assets are present.
    const produced = readdirSync(directory).sort();
    if (produced.length !== 12) {
      throw new Error('Self-test expected 12 assets, found ' + produced.length);
    }

    // 2. The manifest carries the three platform keys with signatures as content.
    const manifest = JSON.parse(readFileSync(join(directory, 'latest.json'), 'utf8'));
    if (manifest.version !== version) {
      throw new Error('Self-test manifest version mismatch');
    }
    for (const key of ['darwin-aarch64', 'windows-x86_64', 'linux-x86_64']) {
      const platform = manifest.platforms[key];
      if (!platform || typeof platform.signature !== 'string' || !platform.url) {
        throw new Error('Self-test manifest is missing platform entry: ' + key);
      }
      if (!platform.signature.startsWith('untrusted comment:')) {
        throw new Error('Self-test signature for ' + key + ' is not a minisign block');
      }
      if (!platform.url.startsWith('https://github.com/' + repository + '/releases/download/')) {
        throw new Error('Self-test URL for ' + key + ' does not point at the release');
      }
    }
    // RFC 3339 / ISO 8601 UTC timestamp.
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/.test(manifest.pub_date)) {
      throw new Error('Self-test pub_date is not RFC 3339: ' + manifest.pub_date);
    }

    // 3. SHA256SUMS.txt covers every asset except itself.
    const checksumLines = readFileSync(join(directory, 'SHA256SUMS.txt'), 'utf8')
      .trim()
      .split('\n');
    if (checksumLines.length !== 11) {
      throw new Error('Self-test checksum list should hold 11 entries, found ' + checksumLines.length);
    }
    for (const line of checksumLines) {
      const [digest, name] = line.split('  ');
      if (!/^[0-9a-f]{64}$/.test(digest)) {
        throw new Error('Self-test checksum is not a SHA-256 hex digest: ' + line);
      }
      const expected = createHash('sha256')
        .update(readFileSync(join(directory, name)))
        .digest('hex');
      if (expected !== digest) {
        throw new Error('Self-test checksum mismatch for ' + name);
      }
    }

    // 4. A truncated signature must be rejected.
    writeFileSync(
      join(directory, 'PolySaver_' + version + '_Linux_x64.AppImage.sig'),
      'untrusted comment: broken\n',
    );
    let rejected = false;
    try {
      generateReleaseMetadata(directory, version, tag, repository);
    } catch {
      rejected = true;
    }
    if (!rejected) {
      throw new Error('Self-test expected a malformed signature to be rejected');
    }

    // 5. A missing asset must be rejected.
    writeFileSync(
      join(directory, 'PolySaver_' + version + '_Linux_x64.AppImage.sig'),
      fakeMinisignSignature('linux appimage'),
    );
    rmSync(join(directory, 'PolySaver_' + version + '_Windows_x64.msi'));
    rejected = false;
    try {
      generateReleaseMetadata(directory, version, tag, repository);
    } catch {
      rejected = true;
    }
    if (!rejected) {
      throw new Error('Self-test expected a missing asset to be rejected');
    }

    console.log('Self-test passed: manifest, checksums and failure paths behave as expected.');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

const args = process.argv.slice(2);

if (args[0] === SELF_TEST_FLAG) {
  selfTest();
} else {
  const [releaseDirectoryArg, version, tag, repository] = args;

  if (!releaseDirectoryArg || !version || !tag || !repository) {
    throw new Error(
      'Usage: node scripts/generate-release-metadata.mjs <release-dir> <version> <tag> <owner/repository>\n' +
        '       node scripts/generate-release-metadata.mjs ' +
        SELF_TEST_FLAG,
    );
  }

  generateReleaseMetadata(releaseDirectoryArg, version, tag, repository);
}

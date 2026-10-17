export const VERSION = __ARCA_RELEASE__.version;
export const STARS = __ARCA_STARS__;
export const SITE_URL = 'https://arca.beyondhumane.com';
export const REPO_URL = 'https://github.com/beyondhumane/arca';
export const ISSUES_URL = `${REPO_URL}/issues`;
export const NEW_ISSUE_URL = `${REPO_URL}/issues/new`;
export const PULLS_URL = `${REPO_URL}/pulls`;
export const RELEASES_URL = `${REPO_URL}/releases`;
export const RELEASE_URL = `${REPO_URL}/releases/tag/v${VERSION}`;
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;
export const CONTRIBUTORS_URL = `${REPO_URL}/graphs/contributors`;

export const CLONE_CMD = 'git clone https://github.com/beyondhumane/arca && cd arca && cargo build --release';

const DL = `${REPO_URL}/releases/download/v${VERSION}`;

export type Platform = 'windows' | 'macos' | 'macos-intel' | 'linux';

export type DownloadAsset = {
  os: string;
  label: string;
  file: string;
  url: string;
  /** Formatted size, e.g. "7.3 MiB", or empty when the build has no release data. */
  size: string;
};

function asset(os: string, label: string, file: string): DownloadAsset {
  const bytes = __ARCA_RELEASE__.sizes[file];
  return { os, label, file, url: `${DL}/${file}`, size: bytes ? `${(bytes / 1_048_576).toFixed(1)} MiB` : '' };
}

export const DOWNLOADS: Record<Platform, DownloadAsset> = {
  windows: asset('Windows', 'Windows', `arca-setup-${VERSION}-x86_64.exe`),
  macos: asset('macOS', 'macOS', `arca-v${VERSION}-macos-arm64.tar.gz`),
  'macos-intel': asset('macOS', 'macOS Intel', `arca-v${VERSION}-macos-x86_64.tar.gz`),
  linux: asset('Linux', 'Linux', `arca-v${VERSION}-linux-x86_64.tar.gz`),
};

/** The rest of the release: Linux packages and the other archives, after the four main downloads. */
export const PACKAGES: DownloadAsset[] = [
  asset('Linux', 'AppImage x86_64', `arca-v${VERSION}-linux-x86_64.AppImage`),
  asset('Linux', 'AppImage arm64', `arca-v${VERSION}-linux-arm64.AppImage`),
  asset('Linux', '.deb amd64', `arca_${VERSION}-1_amd64.deb`),
  asset('Linux', '.deb arm64', `arca_${VERSION}-1_arm64.deb`),
  asset('Linux', '.rpm x86_64', `arca-${VERSION}-1.x86_64.rpm`),
  asset('Linux', '.rpm aarch64', `arca-${VERSION}-1.aarch64.rpm`),
  asset('Linux', 'Linux arm64 tar.gz', `arca-v${VERSION}-linux-arm64.tar.gz`),
  asset('Windows', 'Windows portable zip', `arca-v${VERSION}-windows-x86_64.zip`),
];

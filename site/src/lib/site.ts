export const VERSION = __ARCA_RELEASE__.version;
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
  /** Formatted size, e.g. "7.3 MB", or empty when the build has no release data. */
  size: string;
};

function asset(os: string, label: string, file: string): DownloadAsset {
  const bytes = __ARCA_RELEASE__.sizes[file];
  return { os, label, file, url: `${DL}/${file}`, size: bytes ? `${(bytes / 1e6).toFixed(1)} MB` : '' };
}

export const DOWNLOADS: Record<Platform, DownloadAsset> = {
  windows: asset('Windows', 'Windows', `arca-setup-${VERSION}-x86_64.exe`),
  macos: asset('macOS', 'macOS', `arca-v${VERSION}-macos-arm64.tar.gz`),
  'macos-intel': asset('macOS', 'macOS Intel', `arca-v${VERSION}-macos-x86_64.tar.gz`),
  linux: asset('Linux', 'Linux', `arca-v${VERSION}-linux-x86_64.tar.gz`),
};

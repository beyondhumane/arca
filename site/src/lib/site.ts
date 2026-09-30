export const VERSION = '0.7.2';
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
  detail: string;
};

export const DOWNLOADS: Record<Platform, DownloadAsset> = {
  windows: {
    os: 'Windows',
    label: 'Windows',
    file: `arca-setup-${VERSION}-x86_64.exe`,
    url: `${DL}/arca-setup-${VERSION}-x86_64.exe`,
    detail: 'Installer · x86_64 · 7.3 MB',
  },
  macos: {
    os: 'macOS',
    label: 'macOS',
    file: `arca-v${VERSION}-macos-arm64.tar.gz`,
    url: `${DL}/arca-v${VERSION}-macos-arm64.tar.gz`,
    detail: 'Apple silicon · arm64 · 6.7 MB',
  },
  'macos-intel': {
    os: 'macOS',
    label: 'macOS Intel',
    file: `arca-v${VERSION}-macos-x86_64.tar.gz`,
    url: `${DL}/arca-v${VERSION}-macos-x86_64.tar.gz`,
    detail: 'Intel · x86_64 · 7.5 MB',
  },
  linux: {
    os: 'Linux',
    label: 'Linux',
    file: `arca-v${VERSION}-linux-x86_64.tar.gz`,
    url: `${DL}/arca-v${VERSION}-linux-x86_64.tar.gz`,
    detail: 'x86_64 · 13.8 MB',
  },
};

import type { ComponentType, SVGProps } from 'react';
import { Download } from 'lucide-react';
import { useOS } from '@/lib/hooks';
import type { OS } from '@/lib/hooks';
import { DOWNLOADS } from '@/lib/site';
import type { DownloadAsset, Platform } from '@/lib/site';
import { ButtonLink } from './ui';
import type { ButtonSize, ButtonVariant } from './ui';
import { AppleIcon, LinuxIcon, WindowsIcon } from './icons';

type IconType = ComponentType<SVGProps<SVGSVGElement>>;

export const PLATFORM_ICONS: Record<Platform, IconType> = {
  windows: WindowsIcon,
  macos: AppleIcon,
  'macos-intel': AppleIcon,
  linux: LinuxIcon,
};

export function platformForOS(os: OS): Platform | null {
  if (os === 'windows') return 'windows';
  if (os === 'macos') return 'macos';
  if (os === 'linux') return 'linux';
  return null;
}

export function useRecommendedDownload(): { platform: Platform | null; asset: DownloadAsset | null } {
  const os = useOS();
  const platform = platformForOS(os);
  return { platform, asset: platform ? DOWNLOADS[platform] : null };
}

export function DownloadButton({
  size = 'lg',
  variant = 'brand',
  className,
}: {
  size?: ButtonSize;
  variant?: ButtonVariant;
  className?: string;
}) {
  const { platform, asset } = useRecommendedDownload();
  const Icon: IconType = platform ? PLATFORM_ICONS[platform] : Download;

  return (
    <ButtonLink
      href={asset ? asset.url : '#download'}
      variant={variant}
      size={size}
      className={className}
      aria-label={asset ? `Download Arca for ${asset.os} (${asset.file})` : 'Download Arca'}
    >
      <Icon className="size-[18px]" />
      {asset ? `Download for ${asset.os}` : 'Download Arca'}
    </ButtonLink>
  );
}

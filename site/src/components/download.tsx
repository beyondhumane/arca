import type { ComponentType, SVGProps } from 'react';
import { Download } from 'lucide-react';
import { useOS } from '@/lib/hooks';
import type { OS } from '@/lib/hooks';
import { DOWNLOADS } from '@/lib/site';
import type { DownloadAsset, Platform } from '@/lib/site';
import { useCopy } from '@/lib/i18n';
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
  const t = useCopy({
    en: { forOS: (os: string) => `Download for ${os}`, label: (os: string, f: string) => `Download Arca for ${os} (${f})`, any: 'Download Arca' },
    es: { forOS: (os: string) => `Descargar para ${os}`, label: (os: string, f: string) => `Descargar Arca para ${os} (${f})`, any: 'Descargar Arca' },
  });

  return (
    <ButtonLink
      href={asset ? asset.url : '#download'}
      variant={variant}
      size={size}
      className={className}
      aria-label={asset ? t.label(asset.os, asset.file) : t.any}
    >
      <Icon className="size-[18px]" />
      {asset ? t.forOS(asset.os) : t.any}
    </ButtonLink>
  );
}

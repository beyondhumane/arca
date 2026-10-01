import type { SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement>;

export function GitHubIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M12 .5C5.73.5.5 5.73.5 12c0 5.08 3.29 9.39 7.86 10.91.58.11.79-.25.79-.56v-2.17c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.15.08 1.76 1.19 1.76 1.19 1.03 1.76 2.69 1.25 3.35.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.84 1.19 3.1 0 4.42-2.69 5.39-5.25 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.73 18.27.5 12 .5Z" />
    </svg>
  );
}

export function WindowsIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M3 5.1 10.4 4v7.3H3V5.1Zm8.4-1.2L21 2.5v8.8h-9.6V3.9ZM3 12.7h7.4V20L3 18.9v-6.2Zm8.4 0H21v8.8l-9.6-1.4v-7.4Z" />
    </svg>
  );
}

export function AppleIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path d="M16.37 12.7c-.02-2.2 1.8-3.26 1.88-3.31-1.03-1.5-2.62-1.7-3.19-1.73-1.36-.14-2.65.8-3.34.8-.69 0-1.75-.78-2.88-.76-1.48.02-2.85.86-3.61 2.19-1.54 2.67-.39 6.62 1.11 8.79.73 1.06 1.6 2.25 2.75 2.2 1.1-.04 1.52-.71 2.85-.71 1.33 0 1.71.71 2.88.69 1.19-.02 1.94-1.08 2.66-2.14.84-1.23 1.19-2.42 1.21-2.48-.03-.01-2.31-.89-2.33-3.53Zm-2.19-6.47c.61-.74 1.02-1.76.91-2.78-.88.04-1.94.58-2.57 1.32-.56.65-1.06 1.7-.93 2.7.98.08 1.98-.5 2.59-1.24Z" />
    </svg>
  );
}

export function LinuxIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" {...props}>
      <path
        fillRule="evenodd"
        d="M12 2.2c-2.3 0-3.7 1.9-3.7 4.4 0 1.2.2 2-.5 3.1C6.6 11.4 5 13.6 5 16.3c0 .9.2 1.7.6 2.3-.6.4-.9 1-.6 1.6.5 1 2.1.8 3.3 1 1 .2 1.9.8 3.7.8s2.7-.6 3.7-.8c1.2-.2 2.8 0 3.3-1 .3-.6 0-1.2-.6-1.6.4-.6.6-1.4.6-2.3 0-2.7-1.6-4.9-2.8-6.6-.7-1.1-.5-1.9-.5-3.1 0-2.5-1.4-4.4-3.7-4.4Zm-1.6 3.5c.5 0 .8.5.8 1.1s-.3 1.1-.8 1.1-.8-.5-.8-1.1.3-1.1.8-1.1Zm3.2 0c.5 0 .8.5.8 1.1s-.3 1.1-.8 1.1-.8-.5-.8-1.1.3-1.1.8-1.1ZM12 8.6c.9 0 1.9.5 1.9 1s-1 1.2-1.9 1.2-1.9-.7-1.9-1.2.9-1 1.9-1Z"
      />
    </svg>
  );
}

export function RustIcon(props: IconProps) {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} aria-hidden="true" {...props}>
      <circle cx="12" cy="12" r="7.2" />
      <circle cx="12" cy="12" r="9.6" strokeDasharray="1.6 2.2" />
      <path d="M9 15.5v-7h3.6a2 2 0 0 1 0 4H9m3.2 0 2.3 3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

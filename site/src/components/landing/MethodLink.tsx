import { ArrowRight } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useCopy, useLang } from '@/lib/i18n';
import { docPath } from '@/lib/router';

export function MethodLink({ section, className }: { section: string; className?: string }) {
  const { lang } = useLang();
  const label = useCopy({ en: 'How it was measured', es: 'Cómo se midió' });
  return (
    <a
      href={docPath(lang, 'benchmarks', section)}
      className={cn(
        'mt-4 inline-flex items-center gap-1 text-[11px] text-slate-500 underline decoration-white/15 underline-offset-4 transition hover:text-brand-200 hover:decoration-brand-400',
        className,
      )}
    >
      {label} <ArrowRight className="size-3" />
    </a>
  );
}

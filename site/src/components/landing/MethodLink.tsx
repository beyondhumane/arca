import { ArrowRight } from 'lucide-react';
import { cn } from '@/utils/cn';
import { useCopy } from '@/lib/i18n';

export function MethodLink({ section, className }: { section: string; className?: string }) {
  const label = useCopy({ en: 'How it was measured', es: 'Cómo se midió' });
  return (
    <a
      href={`#/docs/benchmarks/${section}`}
      className={cn(
        'mt-4 inline-flex items-center gap-1 text-[11px] text-slate-500 underline decoration-white/15 underline-offset-4 transition hover:text-brand-200 hover:decoration-brand-400',
        className,
      )}
    >
      {label} <ArrowRight className="size-3" />
    </a>
  );
}

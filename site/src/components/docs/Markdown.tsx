import { marked } from 'marked';
import type { Token, Tokens } from 'marked';
import type { ReactNode } from 'react';
import { A, C, Callout, CodeBlock, DocTable, H2, H3, Keys, LI, P, Step, Steps, Strong, UL } from './primitives';
import type { Lang } from './primitives';
import { headingSlug } from './registry';

const LANGS: Record<string, Lang> = { sh: 'bash', bash: 'bash', text: 'text', yaml: 'yaml', toml: 'toml', rust: 'rust' };
const CALLOUTS = { NOTE: 'note', TIP: 'tip', WARNING: 'warning' } as const;

function href(url: string) {
  const doc = /^([a-z0-9-]+)\.md$/.exec(url);
  return doc ? `#/docs/${doc[1]}` : url;
}

function inline(tokens: Token[]): ReactNode[] {
  const out: ReactNode[] = [];
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    switch (t.type) {
      case 'strong':
        out.push(<Strong key={i}>{inline((t as Tokens.Strong).tokens)}</Strong>);
        break;
      case 'em':
        out.push(<em key={i}>{inline((t as Tokens.Em).tokens)}</em>);
        break;
      case 'codespan':
        out.push(<C key={i}>{(t as Tokens.Codespan).text}</C>);
        break;
      case 'link':
        out.push(
          <A key={i} href={href((t as Tokens.Link).href)}>
            {inline((t as Tokens.Link).tokens)}
          </A>,
        );
        break;
      case 'html': {
        // Only <kbd>…</kbd> is allowed; any other raw HTML is shown as text.
        const end = tokens.findIndex((x, j) => j > i && x.type === 'html' && x.raw === '</kbd>');
        if (t.raw === '<kbd>' && end > i) {
          out.push(<Keys key={i} combo={tokens.slice(i + 1, end).map((x) => x.raw).join('')} />);
          i = end;
        } else {
          out.push(t.raw);
        }
        break;
      }
      case 'br':
        out.push(<br key={i} />);
        break;
      case 'text':
        out.push('tokens' in t && t.tokens ? inline(t.tokens) : (t as Tokens.Text).text);
        break;
      default:
        out.push('text' in t ? (t as Tokens.Generic).text : t.raw);
    }
  }
  return out;
}

const HEADING_IDS = new WeakMap<Token, string>();

function block(t: Token, key: number): ReactNode {
  switch (t.type) {
    case 'heading': {
      const h = t as Tokens.Heading;
      const H = h.depth <= 2 ? H2 : H3;
      return (
        <H key={key} id={HEADING_IDS.get(t) ?? headingSlug(h.text)}>
          {inline(h.tokens)}
        </H>
      );
    }
    case 'paragraph':
      return (
        <P key={key}>
          {inline((t as Tokens.Paragraph).tokens)}
        </P>
      );
    case 'text':
      return <span key={key}>{inline((t as Tokens.Text).tokens ?? [t])}</span>;
    case 'code': {
      const c = t as Tokens.Code;
      return <CodeBlock key={key} code={c.text} lang={LANGS[c.lang ?? ''] ?? 'text'} />;
    }
    case 'table': {
      const tb = t as Tokens.Table;
      return (
        <DocTable
          key={key}
          head={tb.header.map((c) => inline(c.tokens))}
          rows={tb.rows.map((r) => r.map((c) => inline(c.tokens)))}
        />
      );
    }
    case 'list': {
      const l = t as Tokens.List;
      if (l.ordered) {
        return (
          <Steps key={key}>
            {l.items.map((item, i) => {
              const [head, ...rest] = item.tokens;
              return (
                <Step key={i} title={head && 'text' in head ? head.text.replace(/\*\*/g, '') : ''}>
                  {rest.map(block)}
                </Step>
              );
            })}
          </Steps>
        );
      }
      return (
        <UL key={key}>
          {l.items.map((item, i) => (
            <LI key={i}>{item.tokens.map(block)}</LI>
          ))}
        </UL>
      );
    }
    case 'blockquote': {
      const m = /^\[!(NOTE|TIP|WARNING)\]\n(?:\*\*(.+?)\*\*\n)?/.exec((t as Tokens.Blockquote).text);
      if (!m) return <P key={key}>{(t as Tokens.Blockquote).text}</P>;
      const body = marked.lexer((t as Tokens.Blockquote).text.slice(m[0].length));
      return (
        <Callout key={key} type={CALLOUTS[m[1] as keyof typeof CALLOUTS]} title={m[2]}>
          {body.map((b, i) =>
            b.type === 'paragraph' ? <span key={i}>{inline((b as Tokens.Paragraph).tokens)}</span> : block(b, i),
          )}
        </Callout>
      );
    }
    default:
      return null;
  }
}

/** `ids` keeps section anchors stable across languages: the nth heading gets the nth id. */
export function Markdown({ source, ids }: { source: string; ids?: string[] }) {
  const tokens = marked.lexer(source);
  const headings = tokens.filter((t) => t.type === 'heading' && (t as Tokens.Heading).depth <= 3);
  if (ids && ids.length === headings.length) headings.forEach((h, i) => HEADING_IDS.set(h, ids[i]));
  return <>{tokens.map(block)}</>;
}

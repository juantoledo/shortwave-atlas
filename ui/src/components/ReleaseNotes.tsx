// Release notes, drawn from the blocks the core parsed out of the release's Markdown. Links open
// in a new tab (the system browser on desktop: api/links.ts).

import type { NoteBlock } from '../types/generated/NoteBlock';
import type { NoteSpan } from '../types/generated/NoteSpan';

function Spans({ spans }: { spans: NoteSpan[] }) {
  return spans.map((s, i) =>
    s.kind === 'strong' ? <b key={i}>{s.text}</b>
    : s.kind === 'em' ? <i key={i}>{s.text}</i>
    : s.kind === 'code' ? <code key={i}>{s.text}</code>
    : s.kind === 'link' ? <a key={i} href={s.url} target="_blank" rel="noreferrer">{s.text}</a>
    : s.text,
  );
}

export function ReleaseNotes({ blocks, full = false }: { blocks: NoteBlock[]; full?: boolean }) {
  return (
    <div className={'release-notes' + (full ? ' full' : '')}>
      {blocks.map((b, i) =>
        // the popover's own title is the h2: the release's version heading and its sections sit below it
        b.kind === 'heading' ? (b.level <= 2 ? <h3 key={i}><Spans spans={b.text} /></h3> : <h4 key={i}><Spans spans={b.text} /></h4>)
        : b.kind === 'para' ? <p key={i}><Spans spans={b.text} /></p>
        : <ul key={i}>{b.items.map((item, j) => <li key={j}><Spans spans={item} /></li>)}</ul>,
      )}
    </div>
  );
}

import React, { useEffect, useLayoutEffect, useRef, useState } from 'react';
import './live-page.css';

export interface TextSpanDto { start: number; end: number; key: string; value: string }
export interface PageBlockDto { id: string; kind: string; level: number; indent: number; number: number; checked: boolean; lang: string; url: string; alt: string; text: string; spans: TextSpanDto[] }
export interface PageDto { item_id: string; space_id: string; title: string; path: string; version: number; ready: boolean; unsaved: boolean; pending_sync: boolean; save_error: string | null; can_edit: boolean; content_hash: string; edit_context: string; blocks: PageBlockDto[] }
export interface PageSaveRequest { item_id: string; edit_id: string; edit_context: string; base_blocks: PageBlockDto[]; blocks: PageBlockDto[]; order: string[]; changed: PageBlockDto[] }
export interface LivePageProps { page: PageDto; save: (request: PageSaveRequest) => Promise<PageDto>; refresh: () => Promise<PageDto>; onConflict?: (remote: PageDto) => void; onDirtyChange?: (dirty: boolean) => void }
const emptyBlock = (): PageBlockDto => ({ id: crypto.randomUUID(), kind: 'paragraph', level: 0, indent: 0, number: 0, checked: false, lang: '', url: '', alt: '', text: '', spans: [] });
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const safeLink = (url: string) => /^(https?:|mailto:)/i.test(url);

// Edits are text diffs in UTF-16, the same offsets used by native TextSpanDto.
function editText(block: PageBlockDto, text: string): PageBlockDto {
  const old = block.text; let start = 0; while (start < old.length && start < text.length && old[start] === text[start]) start++;
  let end = old.length; let nextEnd = text.length;
  while (end > start && nextEnd > start && old[end-1] === text[nextEnd-1]) { end--; nextEnd--; }
  const delta = nextEnd-end;
  const spans = block.spans.map(s => ({ ...s, start: s.start <= start ? s.start : s.start >= end ? s.start+delta : start, end: s.end <= start ? s.end : s.end >= end ? s.end+delta : nextEnd })).filter(s => s.end > s.start);
  return { ...block, text, spans };
}
function EditableBlock({ block, editable, change, select }: { block: PageBlockDto; editable: boolean; change: (b: PageBlockDto) => void; select: (id: string, start: number, end: number) => void }) {
  const ref = useRef<HTMLElement>(null);
  const [composing,setComposing] = useState(false);
  useLayoutEffect(() => {
    const el = ref.current; if (!el || composing) return;
    // The browser owns text nodes while typing; React must not reconcile mutated children.
    const selection = window.getSelection(); const ranges = selection?.rangeCount && el.contains(selection.anchorNode) ? selection.getRangeAt(0).cloneRange() : null;
    let offset = 0;
    if (ranges) { const before = ranges.cloneRange(); before.selectNodeContents(el); before.setEnd(ranges.startContainer,ranges.startOffset); offset = before.toString().length; }
    const nodes: Node[] = [];
    const points = [...new Set([0,block.text.length,...block.spans.flatMap(s => [Math.max(0,Math.min(block.text.length,s.start)),Math.max(0,Math.min(block.text.length,s.end))])])].sort((a,b)=>a-b);
    points.slice(0,-1).forEach((start,i) => {
      const end=points[i+1] ?? block.text.length; let node:Node=document.createTextNode(block.text.slice(start,end));
      for(const span of block.spans.filter(s=>s.start<=start && s.end>=end)) {
        const tag: Record<string,string>={b:'strong',i:'em',s:'s',c:'code',a:'a'};
        const elementTag=tag[span.key];
        if(!elementTag || (span.key==='a' && !safeLink(span.value))) continue;
        const wrapper=document.createElement(elementTag); if(span.key==='a') { wrapper.setAttribute('href',span.value); wrapper.setAttribute('rel','noreferrer'); } wrapper.append(node);node=wrapper;
      }
      nodes.push(node);
    });
    el.replaceChildren(...nodes);
    if(ranges && selection) { const walker=document.createTreeWalker(el,4); let node=walker.nextNode(); let remaining=Math.min(offset,block.text.length); while(node && remaining>(node.textContent?.length ?? 0)) { remaining-=node.textContent?.length ?? 0;node=walker.nextNode(); } const r=document.createRange(); if(node) r.setStart(node,remaining); else r.selectNodeContents(el); r.collapse(true);selection.removeAllRanges();selection.addRange(r); }
  },[block.text,block.spans,block.kind,composing]);
  const selection = () => {
    const s = window.getSelection(); const el = ref.current;
    if (!s?.rangeCount || !el || !el.contains(s.anchorNode) || !el.contains(s.focusNode)) return;
    const r = s.getRangeAt(0); const before = r.cloneRange(); before.selectNodeContents(el); before.setEnd(r.startContainer,r.startOffset);
    select(block.id,before.toString().length,before.toString().length+r.toString().length);
  };
  const tag = block.kind === 'heading' ? `h${Math.min(6,Math.max(1,block.level))}` : block.kind === 'quote' ? 'blockquote' : block.kind === 'code' ? 'pre' : 'p';
  if (block.kind === 'divider') return <hr />;
  if (block.kind === 'image') return <figure>{safeLink(block.url) ? <img src={block.url} alt={block.alt} /> : <span>{block.alt || 'Image unavailable'}</span>}</figure>;
  const editor = React.createElement(tag, { ref, role: 'textbox', 'aria-label': `Page ${block.kind}`, 'aria-multiline': true, contentEditable: editable, suppressContentEditableWarning: true, onMouseUp: selection, onKeyUp: selection, onFocus: selection,
    onCompositionStart: () => setComposing(true),
    onCompositionEnd: (e: React.CompositionEvent<HTMLElement>) => { change(editText(block,(e.currentTarget.textContent ?? '').replace(/\n/g,'\u2028'))); setComposing(false); selection(); },
    onInput: (e: React.FormEvent<HTMLElement>) => { if(composing) return; change(editText(block,(e.currentTarget.textContent ?? '').replace(/\n/g,'\u2028'))); selection(); },
    onPaste: (e: React.ClipboardEvent<HTMLElement>) => { e.preventDefault(); const s = window.getSelection(); if (!s?.rangeCount || !ref.current?.contains(s.getRangeAt(0).commonAncestorContainer)) return; const r = s.getRangeAt(0); r.deleteContents(); const n = document.createTextNode(e.clipboardData.getData('text/plain').replace(/\r?\n/g,'\u2028')); r.insertNode(n); r.setStartAfter(n); r.collapse(true); s.removeAllRanges(); s.addRange(r); change(editText(block,ref.current.textContent ?? '')); selection(); },
    onKeyDown: (e: React.KeyboardEvent<HTMLElement>) => { if (e.key !== 'Enter' || e.nativeEvent.isComposing) return; e.preventDefault(); const s=window.getSelection(); if(!s?.rangeCount || !ref.current?.contains(s.anchorNode)) return; const r=s.getRangeAt(0);r.deleteContents();const n=document.createTextNode('\u2028');r.insertNode(n);r.setStartAfter(n);r.collapse(true);s.removeAllRanges();s.addRange(r);change(editText(block,ref.current.textContent ?? ''));selection(); },
    onDrop: (e: React.DragEvent) => e.preventDefault(),
  });
  if (['bullet','numbered','task'].includes(block.kind)) return <div className="live-page-list" style={{ paddingInlineStart: `${block.indent*22}px` }}><span aria-hidden={block.kind !== 'task'}>{block.kind === 'task' ? <input aria-label="Task complete" type="checkbox" disabled={!editable} checked={block.checked} onChange={e => change({...block,checked:e.target.checked})} /> : block.kind === 'numbered' ? `${block.number || 1}.` : '•'}</span>{editor}</div>;
  return editor;
}

/** Parent owns loading and real CRDT persistence. Mount with key={page.item_id}. */
export function LivePage({ page, save, refresh, onConflict, onDirtyChange }: LivePageProps) {
  const [base,setBase] = useState(page); const [blocks,setBlocks] = useState(page.blocks);
  const [pending,setPending] = useState(false); const [error,setError] = useState<string | null>(page.save_error);
  const [remote,setRemote] = useState<PageDto | null>(null); const [active,setActive] = useState<{id:string;start:number;end:number} | null>(null);
  const [link,setLink] = useState(''); const [linking,setLinking] = useState(false);
  const [composing,setComposing] = useState(false);
  const editId = useRef(crypto.randomUUID()); const dirty = !same(blocks,base.blocks);
  useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);
  const observed = useRef(page);
  useEffect(() => {
    if (page.item_id !== base.item_id || page.edit_context === observed.current.edit_context) return;
    observed.current=page;
    if (page.edit_context === base.edit_context) return;
    if (dirty || pending) { setRemote(page); onConflict?.(page); }
    else { setBase(page); setBlocks(page.blocks); setError(page.save_error); }
  },[page,base.item_id,base.edit_context,dirty,pending,onConflict]);
  const editable = page.can_edit && page.ready && !pending && page.item_id === base.item_id;
  const replace = (b: PageBlockDto) => { if (editable) { editId.current=crypto.randomUUID(); setBlocks(old => old.map(x => x.id === b.id ? b : x)); } };
  const format = (kind: string) => { const b = blocks.find(b => b.id === active?.id); if (b) replace({...b,kind,level:kind === 'heading' ? 1 : 0}); };
  const mark = (key: string, value = '') => { const b = blocks.find(b => b.id === active?.id); if (!b || !active || active.end <= active.start) return; const exists = b.spans.some(s => s.key === key && s.start === active.start && s.end === active.end); replace({...b,spans:exists ? b.spans.filter(s => !(s.key === key && s.start === active.start && s.end === active.end)) : [...b.spans,{start:active.start,end:active.end,key,value}]}); };
  const persist = async () => {
    if (!editable || !dirty || composing) return; setPending(true); setError(null);
    try {
      const result = await save({item_id:base.item_id,edit_id:editId.current,edit_context:base.edit_context,base_blocks:base.blocks,blocks,order:blocks.map(b => b.id),changed:blocks.filter(b => !same(b,base.blocks.find(x => x.id === b.id)))});
      if (result.item_id !== base.item_id) throw Error('Save returned a different page');
      if (result.save_error) throw Error(result.save_error);
      setBase(result); setBlocks(result.blocks); setRemote(null); editId.current=crypto.randomUUID();
    } catch (e) { setError(e instanceof Error ? e.message : 'Could not save page'); }
    finally { setPending(false); }
  };
  const reload = async () => { setPending(true); setError(null); try { const next = await refresh(); if(next.item_id !== base.item_id) throw Error('Refresh returned a different page'); setBase(next); setBlocks(next.blocks); setRemote(null); editId.current=crypto.randomUUID(); } catch(e) { setError(e instanceof Error ? e.message : 'Could not refresh page'); } finally { setPending(false); } };
  return <section className="live-page" aria-label={page.title || 'Page'} onCompositionStart={() => setComposing(true)} onCompositionEnd={() => setComposing(false)}>
    <header><h1>{page.title || 'Page'}</h1><span role="status">{pending ? 'Saving…' : !page.can_edit ? 'Read only' : dirty ? 'Unsaved edits' : base.pending_sync ? 'Saved on this device · Sync pending' : base.unsaved ? 'Not committed' : 'Saved'}</span><button disabled={!editable || !dirty || composing} onClick={persist}>Save</button></header>
    {!page.ready && <p role="status">Page is still downloading.</p>}
    {error && <p role="alert">{error} Your edits are still here.</p>}
    {remote && <aside role="alert">This page changed elsewhere. Your edits are preserved; saving merges against their original CRDT context. <button disabled={pending} onClick={reload}>Discard edits and refresh</button></aside>}
    <div className="live-page-body">{blocks.map(block => <EditableBlock key={block.id} block={block} editable={editable} change={replace} select={(id,start,end) => setActive({id,start,end})} />)}{editable && <button className="live-page-add" onClick={() => { editId.current=crypto.randomUUID(); setBlocks([...blocks,emptyBlock()]); }}>Add paragraph</button>}</div>
    <div role="toolbar" aria-label="Page formatting" className="live-page-toolbar" onMouseDown={e => { if ((e.target as HTMLElement).tagName === 'BUTTON') e.preventDefault(); }}>
      {([['Bold','b'],['Italic','i']] as const).map(([label,key]) => <button key={key} disabled={!editable || !active || active.end === active.start} onClick={() => mark(key)}>{label}</button>)}
      {([['Paragraph','paragraph'],['Heading','heading'],['Bullet list','bullet'],['Numbered list','numbered'],['Checklist','task'],['Quote','quote']] as const).map(([label,kind]) => <button key={kind} disabled={!editable || !active} onClick={() => format(kind)}>{label}</button>)}
      <button disabled={!editable || !active || active.end === active.start} onClick={() => setLinking(!linking)}>Link</button>
    </div>
    {linking && <form onSubmit={e => { e.preventDefault(); if (safeLink(link)) { mark('a',link); setLinking(false); setLink(''); } }}><label>Link address<input type="url" value={link} onChange={e => setLink(e.target.value)} required /></label><button disabled={!editable || !safeLink(link)}>Apply link</button><button type="button" onClick={() => setLinking(false)}>Cancel</button></form>}
  </section>;
}

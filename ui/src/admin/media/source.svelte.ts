// Paged, filter-aware media list for the virtual grid. Pages load on demand;
// `version` changes whenever data arrives so the grid re-renders.

import { api, type MediaItem, type MediaPage } from '../../lib/api';

export const PAGE = 200;

export interface Filter {
  q: string;
  category: string; // '' | 'none' | id
  tags: number[];
  untagged: boolean;
  kind: string;
  year: string;
  visibility: string;
  profile: string;
  sort: string;
  library: string;
  folder: string;
}

export const emptyFilter = (): Filter => ({
  q: '',
  category: '',
  tags: [],
  untagged: false,
  kind: '',
  year: '',
  visibility: '',
  profile: '',
  sort: '',
  library: '',
  folder: '',
});

export function filterToParams(f: Filter): URLSearchParams {
  const p = new URLSearchParams();
  if (f.q.trim()) p.set('q', f.q.trim());
  if (f.category === 'none') p.set('uncategorized', 'true');
  else if (f.category) p.set('category', f.category);
  if (f.tags.length) p.set('tags', f.tags.join(','));
  if (f.untagged) p.set('untagged', 'true');
  for (const k of ['kind', 'year', 'visibility', 'profile', 'sort', 'library', 'folder'] as const) {
    if (f[k]) p.set(k, f[k]);
  }
  return p;
}

export function paramsToFilter(p: URLSearchParams): Filter {
  const f = emptyFilter();
  f.q = p.get('q') ?? '';
  f.category = p.get('uncategorized') === 'true' ? 'none' : (p.get('category') ?? '');
  f.tags = (p.get('tags') ?? '').split(',').map(Number).filter((n) => n > 0);
  f.untagged = p.get('untagged') === 'true';
  for (const k of ['kind', 'year', 'visibility', 'profile', 'sort', 'library', 'folder'] as const) {
    f[k] = p.get(k) ?? '';
  }
  return f;
}

export class MediaSource {
  total = $state(0);
  version = $state(0);
  loaded = $state(false);
  private qs = '';
  private pages = new Map<number, MediaItem[]>();
  private pending = new Set<number>();
  private idsCache: Promise<number[]> | null = null;

  setFilter(params: URLSearchParams) {
    this.qs = params.toString();
    this.pages.clear();
    this.pending.clear();
    this.idsCache = null;
    this.loaded = false;
    this.version++;
    this.load(0);
  }

  /** Drops cached pages (after edits); visible pages reload through `ensure`. */
  refresh() {
    this.pages.clear();
    this.pending.clear();
    this.idsCache = null;
    this.version++;
    this.load(0);
  }

  get(index: number): MediaItem | undefined {
    return this.pages.get(Math.floor(index / PAGE))?.[index % PAGE];
  }

  indexOf(id: number): number {
    for (const [p, items] of this.pages) {
      const i = items.findIndex((m) => m.id === id);
      if (i >= 0) return p * PAGE + i;
    }
    return -1;
  }

  ensure(from: number, to: number) {
    for (let p = Math.floor(from / PAGE); p <= Math.floor(Math.max(from, to) / PAGE); p++) this.load(p);
  }

  private async load(page: number) {
    if (this.pages.has(page) || this.pending.has(page)) return;
    if (this.loaded && page * PAGE >= this.total) return;
    this.pending.add(page);
    const qs = this.qs;
    try {
      const res = await api.get<MediaPage>(`/admin/media?${qs}${qs ? '&' : ''}offset=${page * PAGE}&limit=${PAGE}`);
      if (qs !== this.qs) return;
      this.pages.set(page, res.items);
      this.total = res.total;
      this.loaded = true;
      this.version++;
    } finally {
      this.pending.delete(page);
    }
  }

  /** All ids matching the filter, in grid order. */
  allIds(): Promise<number[]> {
    this.idsCache ??= api.get<number[]>(`/admin/media/ids?${this.qs}`);
    return this.idsCache;
  }

  /** Ids for an index range, from loaded pages when possible. */
  async idsBetween(a: number, b: number): Promise<number[]> {
    const [lo, hi] = a < b ? [a, b] : [b, a];
    const out: number[] = [];
    for (let i = lo; i <= hi; i++) {
      const m = this.get(i);
      if (!m) return (await this.allIds()).slice(lo, hi + 1);
      out.push(m.id);
    }
    return out;
  }
}

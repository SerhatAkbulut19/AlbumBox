// Shared admin reference data (categories, tag types, profiles). Loaded once
// and refreshed after edits so every page sees the same lists.

import { api, type Category, type Profile, type TagType } from './api';

export interface CategoryNode extends Category {
  depth: number;
  path: string;
}

class CatalogStore {
  categories = $state<Category[]>([]);
  tagTypes = $state<TagType[]>([]);
  profiles = $state<Profile[]>([]);
  private loaded = false;

  /** Categories in tree order with depth and full path ("Bayramlar / Ramazan 2023"). */
  get tree(): CategoryNode[] {
    const byParent = new Map<number | null, Category[]>();
    for (const c of this.categories) {
      const list = byParent.get(c.parent_id) ?? [];
      list.push(c);
      byParent.set(c.parent_id, list);
    }
    const out: CategoryNode[] = [];
    const walk = (parent: number | null, depth: number, prefix: string) => {
      const children = (byParent.get(parent) ?? []).sort(
        (a, b) => a.sort_order - b.sort_order || a.name.localeCompare(b.name, 'tr'),
      );
      for (const c of children) {
        const path = prefix ? `${prefix} / ${c.name}` : c.name;
        out.push({ ...c, depth, path });
        walk(c.id, depth + 1, path);
      }
    };
    walk(null, 0, '');
    return out;
  }

  typeById(id: number) {
    return this.tagTypes.find((t) => t.id === id);
  }

  typeByCode(code: string) {
    return this.tagTypes.find((t) => t.code === code);
  }

  profileName(id: number) {
    return this.profiles.find((p) => p.id === id)?.name ?? `#${id}`;
  }

  async ensure() {
    if (this.loaded) return;
    this.loaded = true;
    await Promise.all([this.reloadCategories(), this.reloadTagTypes(), this.reloadProfiles()]);
  }

  async reloadCategories() {
    this.categories = await api.get<Category[]>('/admin/categories');
  }

  async reloadTagTypes() {
    this.tagTypes = await api.get<TagType[]>('/admin/tag-types');
  }

  async reloadProfiles() {
    this.profiles = await api.get<Profile[]>('/admin/profiles');
  }
}

export const catalog = new CatalogStore();

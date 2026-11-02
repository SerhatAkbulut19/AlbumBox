// HTTP client. The web UI authenticates with the HttpOnly session cookie;
// the desktop (Tauri) build will add a Bearer token and an API base URL.

export type Role = 'admin' | 'member';

export interface User {
  id: number;
  username: string;
  display_name: string;
  role: Role;
  is_active: boolean;
  created_at: string;
  last_login_at: string | null;
}

export interface AdminStats {
  media: number;
  photos: number;
  videos: number;
  published_media: number;
  persons: number;
  tags: number;
  categories: number;
  users: number;
  profiles: number;
  libraries: number;
  libraries_online: number;
  pending_face_suggestions: number;
}

export interface HomeSummary {
  display_name: string;
  visible_media: number;
  visible_categories: number;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
  ) {
    super(code);
  }
}

const base = (localStorage.getItem('albumbox.apiBase') ?? '').replace(/\/$/, '');

/** Called on 401 so the app can return to the login screen. */
let onUnauthorized: () => void = () => {};
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${base}/api${path}`, {
      method,
      credentials: 'include',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch {
    throw new ApiError(0, 'network');
  }
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    const err = new ApiError(res.status, data.code ?? 'internal');
    if (res.status === 401 && !path.startsWith('/auth/')) onUnauthorized();
    throw err;
  }
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body ?? {}),
  put: <T>(path: string, body?: unknown) => request<T>('PUT', path, body ?? {}),
  del: <T>(path: string) => request<T>('DELETE', path),
};

export interface Library {
  id: string;
  name: string;
  root_subpath: string;
  last_seen_root: string | null;
  is_online: boolean;
  last_scan_at: string | null;
  created_at: string;
  file_count: number;
  media_count: number;
  missing_count: number;
}

export interface FolderSummary {
  folder_rel: string;
  suggestion: string;
  media_count: number;
  sample_media_ids: number[];
  tags: string[];
}

export interface FsListing {
  path: string | null;
  parent: string | null;
  entries: { name: string; path: string; is_library: boolean }[];
}

export interface JobProgress {
  job_id: number;
  kind: string;
  library_id: string | null;
  library_name: string | null;
  phase: '' | 'listing' | 'analyzing' | 'thumbs';
  done: number;
  total: number;
  current: string;
  listed: number;
  unchanged: number;
  new_media: number;
  existing_media: number;
  missing: number;
  errors: number;
  thumbs_done: number;
  thumbs_failed: number;
  faces_found: number;
}

export interface EngineSnapshot {
  current: JobProgress | null;
  queued: number;
  last_finished: { status: string; message: string | null; progress: JobProgress } | null;
  libraries_version: number;
  ffmpeg_available: boolean;
  faces_available: boolean;
}

export const thumbUrl = (id: number, size: 256 | 1024 = 256) => `${base}/api/media/${id}/thumb/${size}`;
export const apiUrl = (path: string) => `${base}/api${path}`;

export interface MediaItem {
  id: number;
  kind: 'photo' | 'video';
  taken_at: string | null;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  thumb_ready: number;
  is_hidden: boolean;
  visibility_mode: 'inherit' | 'custom';
  published: boolean;
  name: string | null;
}

export interface MediaPage {
  total: number;
  offset: number;
  items: MediaItem[];
}

export interface MediaDetail extends MediaItem {
  camera: string | null;
  taken_at_source: string | null;
  gps_lat: number | null;
  gps_lon: number | null;
  files: { library_id: string; library_name: string; library_online: boolean; rel_path: string; size: number; is_missing: boolean }[];
  tags: { id: number; name: string; type_code: string; type_name: string; color: string | null }[];
  categories: [number, string][];
  share_profile_ids: number[];
  access_profile_ids: number[];
}

export interface Category {
  id: number;
  parent_id: number | null;
  name: string;
  description: string | null;
  sort_order: number;
  cover_media_id: number | null;
  media_count: number;
  profile_ids: number[];
}

export interface TagType {
  id: number;
  code: string;
  name: string;
  color: string | null;
  is_builtin: boolean;
  sort_order: number;
  tag_count: number;
}

export interface Tag {
  id: number;
  type_id: number;
  type_code: string;
  name: string;
  parent_id: number | null;
  parent_name: string | null;
  media_count: number;
}

export interface Profile {
  id: number;
  name: string;
  description: string | null;
  is_builtin: boolean;
  member_ids: number[];
  visible_media: number;
}

export type BulkOp =
  | { action: 'add_tags'; tag_ids: number[] }
  | { action: 'remove_tags'; tag_ids: number[] }
  | { action: 'add_to_category'; category_id: number }
  | { action: 'move_to_category'; category_id: number; from_category_id: number | null }
  | { action: 'remove_from_category'; category_id: number }
  | { action: 'set_visibility'; mode: 'inherit' | 'private' | 'profiles'; profile_ids: number[] }
  | { action: 'set_hidden'; hidden: boolean }
  | { action: 'set_taken_at'; taken_at: string };

export const bulk = (ids: number[], op: BulkOp) => api.post<{ changed: number }>('/admin/media/bulk', { ids, ...op });
export const originalUrl = (id: number) => `${base}/api/media/${id}/original`;

// ---- Client (family) views -------------------------------------------------

export interface ClientItem {
  id: number;
  kind: 'photo' | 'video';
  taken_at: string | null;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  thumb_ready: number;
}

export interface ClientPage {
  total: number;
  offset: number;
  items: ClientItem[];
}

export interface Album {
  id: number;
  parent_id: number | null;
  name: string;
  description: string | null;
  media_count: number;
  total_count: number;
  cover_media_id: number | null;
  date_from: string | null;
  date_to: string | null;
}

export interface Home {
  display_name: string;
  visible_media: number;
  albums: Album[];
  album_count: number;
  years: [string, number][];
  recent: ClientItem[];
}

export interface TagRef {
  id: number;
  name: string;
  type_code: string;
  color: string | null;
}

export interface SearchGroup {
  key: string;
  name: string | null;
  count: number;
  cover_media_id: number | null;
  date_from: string | null;
  date_to: string | null;
}

export interface Facet {
  id: number;
  name: string;
  count: number;
}

export interface SearchResult {
  total: number;
  terms: { word: string; tags: TagRef[] }[];
  groups: SearchGroup[];
  facets: { persons: Facet[]; places: Facet[]; events: Facet[]; others: Facet[]; years: [string, number][] };
}

export interface MediaInfo extends ClientItem {
  camera: string | null;
  tags: TagRef[];
  albums: [number, string][];
}

// ---- Faces -------------------------------------------------------------------

export interface FaceRef {
  id: number;
  media_id: number;
  score: number;
}

export interface FaceCluster {
  cluster_id: number;
  count: number;
  media_count: number;
  faces: FaceRef[];
}

export interface PersonSummary {
  tag_id: number;
  name: string;
  confirmed: number;
  suggested: number;
  cover_face_id: number | null;
}

export interface FaceStatus {
  available: boolean;
  models_dir: string;
  pending_media: number;
  analysed_media: number;
  faces: number;
  unassigned: number;
  suggested: number;
  confirmed: number;
}

export interface MediaFace {
  id: number;
  bbox: [number, number, number, number];
  status: 'unassigned' | 'suggested' | 'confirmed' | 'rejected';
  person_tag_id: number | null;
  person_name: string | null;
}

export const faceUrl = (id: number) => `${base}/api/admin/faces/${id}/crop`;

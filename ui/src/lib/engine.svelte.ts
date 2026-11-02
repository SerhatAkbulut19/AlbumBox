// Live engine snapshot over Server-Sent Events (admin only). Reference counted:
// the connection stays open while at least one component uses it.

import { apiUrl, type EngineSnapshot } from './api';

class EngineStore {
  snap = $state<EngineSnapshot | null>(null);
  private source: EventSource | null = null;
  private users = 0;

  get running() {
    return this.snap?.current ?? null;
  }

  connect() {
    this.users++;
    if (this.source) return;
    this.source = new EventSource(apiUrl('/admin/events'), { withCredentials: true });
    this.source.addEventListener('engine', (e) => {
      this.snap = JSON.parse((e as MessageEvent).data);
    });
  }

  disconnect() {
    this.users = Math.max(0, this.users - 1);
    if (this.users === 0) {
      this.source?.close();
      this.source = null;
    }
  }
}

export const engine = new EngineStore();

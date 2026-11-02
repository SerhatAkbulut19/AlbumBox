import { api, setUnauthorizedHandler, type User } from './api';
import { navigate } from './router.svelte';

interface SessionResponse {
  user: User;
  token: string;
}

class Session {
  ready = $state(false);
  needsSetup = $state(false);
  user = $state<User | null>(null);

  get isAdmin() {
    return this.user?.role === 'admin';
  }

  async init() {
    setUnauthorizedHandler(() => {
      this.user = null;
    });
    try {
      const status = await api.get<{ needs_setup: boolean }>('/setup');
      this.needsSetup = status.needs_setup;
      if (!this.needsSetup) {
        const me = await api.get<{ user: User }>('/auth/me').catch(() => null);
        this.user = me?.user ?? null;
      }
    } finally {
      this.ready = true;
    }
  }

  async setup(username: string, display_name: string, password: string) {
    const res = await api.post<SessionResponse>('/setup', { username, display_name, password });
    this.needsSetup = false;
    this.signedIn(res.user);
  }

  async login(username: string, password: string, remember: boolean) {
    const res = await api.post<SessionResponse>('/auth/login', { username, password, remember });
    this.signedIn(res.user);
  }

  async logout() {
    await api.post('/auth/logout').catch(() => {});
    this.user = null;
    navigate('/', true);
  }

  private signedIn(user: User) {
    this.user = user;
    navigate(user.role === 'admin' ? '/admin' : '/', true);
  }
}

export const session = new Session();

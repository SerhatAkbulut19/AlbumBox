type Theme = 'light' | 'dark';

const KEY = 'albumbox.theme';

class ThemeStore {
  current = $state<Theme>(
    (localStorage.getItem(KEY) as Theme | null) ??
      (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'),
  );

  constructor() {
    this.apply();
  }

  toggle() {
    this.current = this.current === 'dark' ? 'light' : 'dark';
    localStorage.setItem(KEY, this.current);
    this.apply();
  }

  private apply() {
    document.documentElement.dataset.theme = this.current;
  }
}

export const theme = new ThemeStore();

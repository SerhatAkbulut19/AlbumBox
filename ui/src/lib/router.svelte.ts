// Minimal history router: `router.path` / `router.search` are reactive,
// `navigate` changes them, `use:link` turns <a href> clicks into client-side navigation.

class Router {
  path = $state(location.pathname);
  search = $state(location.search);
}

export const router = new Router();

export function navigate(to: string, replace = false) {
  const url = new URL(to, location.origin);
  if (url.pathname === router.path && url.search === router.search) return;
  if (replace) history.replaceState(null, '', url.pathname + url.search);
  else history.pushState(null, '', url.pathname + url.search);
  router.path = url.pathname;
  router.search = url.search;
}

/** Updates the query string without a navigation (e.g. filters). */
export function replaceQuery(params: URLSearchParams) {
  const qs = params.toString();
  const search = qs ? `?${qs}` : '';
  history.replaceState(null, '', location.pathname + search);
  router.search = search;
}

window.addEventListener('popstate', () => {
  router.path = location.pathname;
  router.search = location.search;
});

export function link(node: HTMLAnchorElement) {
  const onClick = (e: MouseEvent) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const url = new URL(node.href);
    if (url.origin !== location.origin) return;
    e.preventDefault();
    navigate(url.pathname + url.search);
  };
  node.addEventListener('click', onClick);
  return { destroy: () => node.removeEventListener('click', onClick) };
}

/** True when `path` is `prefix` itself or below it. */
export function isUnder(path: string, prefix: string): boolean {
  return path === prefix || path.startsWith(prefix + '/');
}

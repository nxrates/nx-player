type Theme = 'light' | 'dark' | 'system';

let theme = $state<Theme>((localStorage.getItem('ls-theme') as Theme) || 'system');

function getEffectiveTheme(t: Theme): 'light' | 'dark' {
  if (t === 'system') {
    return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  return t;
}

// Apply theme immediately on load
document.documentElement.setAttribute('data-theme', getEffectiveTheme(theme));

export function getTheme() { return theme; }
export function setTheme(t: Theme) {
  theme = t;
  localStorage.setItem('ls-theme', t);
  document.documentElement.setAttribute('data-theme', getEffectiveTheme(t));
}

export function loadLocal<T>(key: string, fallback: T, validate: (value: unknown) => value is T): T {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(`sound-connect:${key}`) ?? 'null');
    return validate(value) ? value : fallback;
  } catch { return fallback; }
}

export function saveLocal(key: string, value: unknown): boolean {
  try { localStorage.setItem(`sound-connect:${key}`, JSON.stringify(value)); return true; }
  catch { return false; }
}

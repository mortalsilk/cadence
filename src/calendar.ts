export function localDate(date = new Date()): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}
export function parseDate(date: string): Date {
  return new Date(`${date}T12:00:00`);
}
export function calendarDays(offset: number, today = new Date()): string[] {
  const end = new Date(
    today.getFullYear(),
    today.getMonth(),
    today.getDate(),
    12,
  );
  end.setDate(end.getDate() + ((7 - end.getDay()) % 7) - offset * 84);
  const start = new Date(end);
  start.setDate(start.getDate() - 83);
  return Array.from({ length: 84 }, (_, i) => {
    const d = new Date(start);
    d.setDate(d.getDate() + i);
    return localDate(d);
  });
}
export function clock(ms: number): string {
  const seconds = Math.floor(Math.max(0, ms) / 1000);
  const h = Math.floor(seconds / 3600),
    m = Math.floor(seconds / 60) % 60,
    s = seconds % 60;
  return [h, m, s].map((n) => String(n).padStart(2, '0')).join(':');
}
export function duration(ms: number): string {
  const mins = Math.floor(ms / 60000);
  if (!mins) return `${Math.floor(ms / 1000)}s`;
  return mins >= 60 ? `${Math.floor(mins / 60)}h ${mins % 60}m` : `${mins}m`;
}
export function friendly(
  date: string,
  options: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' },
): string {
  return parseDate(date).toLocaleDateString(undefined, options);
}

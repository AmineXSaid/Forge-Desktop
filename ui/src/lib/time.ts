// Relative times for the sidebar.

export function when(ms: number, now = new Date()): string {
  const d = new Date(ms);
  const mins = Math.round((now.getTime() - ms) / 60000);
  if (mins < 1) return 'now';
  if (mins < 60) return `${mins}m`;
  if (d.toDateString() === now.toDateString()) return `${Math.floor(mins / 60)}h`;
  const days = Math.round((startOfDay(now) - startOfDay(d)) / 86400000);
  if (days < 7) return `${days}d`;
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

function startOfDay(d: Date) {
  const c = new Date(d);
  c.setHours(0, 0, 0, 0);
  return c.getTime();
}

export function bucket(ms: number, now = new Date()): string {
  const t = startOfDay(now);
  if (ms >= t) return 'Today';
  if (ms >= t - 86400000) return 'Yesterday';
  if (ms >= t - 7 * 86400000) return 'Previous 7 days';
  if (ms >= t - 30 * 86400000) return 'Previous 30 days';
  return 'Older';
}

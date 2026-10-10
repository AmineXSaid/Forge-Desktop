// One popover menu at a time, anchored to the control that opened it.

export type MenuItem =
  | { label: string; desc?: string; icon?: string; checked?: boolean; danger?: boolean; hint?: string; run: () => void }
  | { header: string }
  | '-';

export const menu = $state<{ items: MenuItem[]; x: number; y: number; above: boolean; anchor: HTMLElement | null; width: number }>({
  items: [], x: 0, y: 0, above: false, anchor: null, width: 240,
});

export function openMenu(anchor: HTMLElement, items: MenuItem[], opts: { align?: 'left' | 'right'; above?: boolean; width?: number } = {}) {
  if (menu.anchor === anchor) return closeMenu();
  const r = anchor.getBoundingClientRect();
  menu.width = opts.width ?? 260;
  menu.items = items;
  menu.anchor = anchor;
  menu.above = !!opts.above;
  menu.x = Math.max(8, Math.min(opts.align === 'right' ? r.right - menu.width : r.left, innerWidth - menu.width - 8));
  menu.y = opts.above ? innerHeight - r.top + 6 : r.bottom + 6;
}

export function closeMenu() {
  menu.items = [];
  menu.anchor = null;
}

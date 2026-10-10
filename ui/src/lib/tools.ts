// How a tool call reads in the thread: an icon, a verb, and its main argument.

export const TOOL_ICON: Record<string, string> = {
  Bash: 'terminal', Read: 'eye', Write: 'file', Edit: 'pencil', MultiEdit: 'pencil', NotebookEdit: 'pencil',
  Grep: 'search', Glob: 'search', WebFetch: 'globe', WebSearch: 'globe', TodoWrite: 'list', Task: 'sparkle', Agent: 'sparkle',
  ExitPlanMode: 'plan', AskUserQuestion: 'question',
};

export const TOOL_VERB: Record<string, string> = {
  Bash: 'Ran', Read: 'Read', Write: 'Wrote', Edit: 'Edited', MultiEdit: 'Edited', NotebookEdit: 'Edited',
  Grep: 'Searched', Glob: 'Found files', WebFetch: 'Fetched', WebSearch: 'Searched the web', TodoWrite: 'Updated the plan',
  Task: 'Agent', Agent: 'Agent',
};

export function relPath(p: string | undefined, workspace: string | null | undefined): string {
  if (!p) return '';
  const ws = (workspace ?? '').replace(/[\\/]+$/, '');
  if (ws && p.toLowerCase().startsWith(ws.toLowerCase())) return p.slice(ws.length).replace(/^[\\/]+/, '').replaceAll('\\', '/') || p;
  return p;
}

export function toolArg(name: string, input: any, workspace: string | null | undefined): string {
  const i = input ?? {};
  switch (name) {
    case 'Bash': return i.command ?? '';
    case 'Read': case 'Write': case 'Edit': case 'MultiEdit': return relPath(i.file_path, workspace);
    case 'NotebookEdit': return relPath(i.notebook_path, workspace);
    case 'Grep': return `${i.pattern ?? ''}${i.path ? `  in ${relPath(i.path, workspace)}` : ''}`;
    case 'Glob': return i.pattern ?? '';
    case 'WebFetch': return i.url ?? '';
    case 'WebSearch': return i.query ?? '';
    case 'Task': case 'Agent': return i.description ?? '';
    case 'TodoWrite': return `${(i.todos ?? []).length} items`;
    default: return (Object.values(i).find((v) => typeof v === 'string') as string) ?? '';
  }
}

const TOOL_ACTIVE: Record<string, string> = { Bash: 'Run', Read: 'Read', Write: 'Write', Edit: 'Edit', MultiEdit: 'Edit', NotebookEdit: 'Edit', Grep: 'Search', Glob: 'Find files', WebFetch: 'Fetch', WebSearch: 'Search the web', TodoWrite: 'Update the plan' };

/** The verb for a call: present while it runs or waits, past once it ran. */
export function toolName(name: string, done = true): string {
  return (done ? TOOL_VERB[name] : TOOL_ACTIVE[name]) ?? name.replace(/^mcp__([^_]+)__/, '$1 · ');
}

export function permissionTitle(tool: string, input: any, workspace: string | null | undefined): string {
  const i = input ?? {};
  switch (tool) {
    case 'Bash': return 'Run this command?';
    case 'Write': return `Create ${relPath(i.file_path, workspace)}?`;
    case 'Edit': case 'MultiEdit': return `Edit ${relPath(i.file_path, workspace)}?`;
    case 'NotebookEdit': return `Edit ${relPath(i.notebook_path, workspace)}?`;
    case 'WebFetch': return `Fetch ${i.url ?? 'a page'}?`;
    case 'WebSearch': return `Search the web for “${i.query ?? ''}”?`;
    case 'ExitPlanMode': return 'Ready to start on this plan?';
    default: return `Use ${tool.replace(/^mcp__([^_]+)__/, '$1 · ')}?`;
  }
}

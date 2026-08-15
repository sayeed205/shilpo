export interface SearchCommand {
  id: string;
  title: string;
  description: string;
}

export function searchCommands(query: string): SearchCommand[] {
  const commands: SearchCommand[] = [
    {
      id: "toggle-power",
      title: "Showcase: Toggle Mode",
      description: "Toggle between active and idle showcase modes",
    },
    {
      id: "increment-counter",
      title: "Showcase: Increment Clicks",
      description: "Increment showcase click counter",
    },
    {
      id: "open-settings",
      title: "Showcase: Preferences",
      description: "Configure showcase extension settings",
    },
  ];

  const q = query.toLowerCase().trim();
  if (q.length === 0) {
    return commands;
  }
  return commands.filter(
    (cmd) => cmd.title.toLowerCase().includes(q) || cmd.description.toLowerCase().includes(q),
  );
}

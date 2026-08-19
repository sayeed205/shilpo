import type { SearchCandidate, SearchRequest } from "@shilpo/ext-sdk";

export function handleSearch(
  contributionId: string,
  request: SearchRequest,
): SearchCandidate[] {
  if (contributionId !== "search-commands") {
    return [];
  }

  const allCandidates: SearchCandidate[] = [
    {
      id: "toggle-power",
      title: "Showcase: Toggle Mode",
      subtitle: "Toggle between active and idle showcase modes",
      aliases: ["power", "mode", "toggle"],
      keywords: ["showcase", "state"],
      category: "action",
      icon: { tag: "named", val: "settings" },
      activationVerb: "Toggle",
      activationPayload: "toggle-power",
    },
    {
      id: "increment-counter",
      title: "Showcase: Increment Clicks",
      subtitle: "Increment showcase click counter",
      aliases: ["click", "counter", "add"],
      keywords: ["showcase", "counter"],
      category: "action",
      icon: { tag: "named", val: "star" },
      activationVerb: "Increment",
      activationPayload: "increment-counter",
    },
    {
      id: "open-settings",
      title: "Showcase: Preferences",
      subtitle: "Configure showcase extension settings",
      aliases: ["settings", "preferences", "config"],
      keywords: ["showcase", "options"],
      category: "action",
      icon: { tag: "named", val: "settings" },
      activationVerb: "Open",
      activationPayload: "open-settings",
    },
  ];

  const q = request.query.toLowerCase().trim();
  if (q.length === 0) {
    return allCandidates;
  }
  return allCandidates.filter(
    (cand) =>
      cand.title.toLowerCase().includes(q) ||
      (cand.subtitle && cand.subtitle.toLowerCase().includes(q)) ||
      cand.aliases.some((a) => a.toLowerCase().includes(q)) ||
      cand.keywords.some((k) => k.toLowerCase().includes(q)),
  );
}

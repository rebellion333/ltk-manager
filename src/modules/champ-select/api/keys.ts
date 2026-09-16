export const champSelectKeys = {
  all: ["champ-select"] as const,
  status: () => [...champSelectKeys.all, "status"] as const,
  preferences: () => [...champSelectKeys.all, "preferences"] as const,
  favorites: () => [...champSelectKeys.all, "favorites"] as const,
  roster: () => [...champSelectKeys.all, "roster"] as const,
  mods: (champion: string) => [...champSelectKeys.all, "mods", champion] as const,
};

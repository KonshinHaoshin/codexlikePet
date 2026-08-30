export interface PetCatalogEntry {
  id: string;
  path: string;
}

interface PetCatalogFile {
  pets?: unknown;
}

/** Load the small public catalog used by the frontend pet switcher. */
export async function loadPetCatalog(baseUrl: string): Promise<PetCatalogEntry[]> {
  const response = await fetch(`${baseUrl}/index.json`);
  if (!response.ok) throw new Error(`cannot fetch pet catalog: ${response.status}`);

  const data = (await response.json()) as PetCatalogFile;
  if (!Array.isArray(data.pets) || data.pets.length === 0) {
    throw new Error("pet catalog must contain at least one pet");
  }

  const ids = new Set<string>();
  return data.pets.map((raw, index) => {
    if (!raw || typeof raw !== "object") {
      throw new Error(`invalid pet catalog entry at index ${index}`);
    }
    const entry = raw as { id?: unknown; path?: unknown };
    if (typeof entry.id !== "string" || entry.id.trim() === "") {
      throw new Error(`pet catalog entry ${index} has no valid id`);
    }
    if (typeof entry.path !== "string" || entry.path.trim() === "") {
      throw new Error(`pet catalog entry ${index} has no valid path`);
    }
    if (ids.has(entry.id)) throw new Error(`duplicate pet id: ${entry.id}`);
    ids.add(entry.id);
    return { id: entry.id, path: entry.path };
  });
}

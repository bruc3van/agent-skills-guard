function deleteIndexedDb(name: string): Promise<void> {
  return new Promise((resolve) => {
    const request = indexedDB.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => resolve();
    request.onblocked = () => resolve();
  });
}

export async function clearWebPersistedData(): Promise<void> {
  try {
    localStorage.clear();
  } catch {
    // ignore
  }

  try {
    sessionStorage.clear();
  } catch {
    // ignore
  }

  try {
    const idbAny = indexedDB as unknown as {
      databases?: () => Promise<Array<{ name?: string | null }>>;
    };
    const databases = await idbAny.databases?.();
    const names = (databases ?? [])
      .map((db) => db.name)
      .filter((name): name is string => typeof name === "string" && name.length > 0);

    await Promise.all(names.map((name) => deleteIndexedDb(name)));
  } catch {
    // ignore
  }

  try {
    const keys = await caches.keys();
    await Promise.all(keys.map((key) => caches.delete(key)));
  } catch {
    // ignore
  }

  try {
    const registrations = await navigator.serviceWorker.getRegistrations();
    await Promise.all(registrations.map((registration) => registration.unregister()));
  } catch {
    // ignore
  }
}

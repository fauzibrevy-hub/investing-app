/**
 * `matchMedia` yang menjawab `(min-width: Npx)` dan `(max-width: Npx)` menurut lebar jendela tiruan, supaya tes
 * menjalankan mode 1280 dan 1440 px. jsdom tidak punya tata letak.
 */
export function setViewport(width: number): void {
  const matches = (query: string): boolean => {
    const min = /\(min-width:\s*(\d+)px\)/.exec(query);
    const max = /\(max-width:\s*(\d+)px\)/.exec(query);
    return (!min || width >= Number(min[1])) && (!max || width <= Number(max[1]));
  };
  window.matchMedia = ((query: string) => ({
    matches: matches(query),
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}

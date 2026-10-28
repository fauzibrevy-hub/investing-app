import { useMemo, useRef, useState } from "react";
import { ApiError, newIdempotencyKey } from "../../../lib/api";
import { dismissEvent } from "../api";
import { formatDate } from "../format";
import type { DeskResponse, EventFilter, ResearchEvent } from "../types";

/** Hari WIB sebagai kunci kelompok; `formatDate` membaca stempel RFC 3339 sebagai hari WIB. */
const dayOf = (event: ResearchEvent) => formatDate(event.at);

export interface UseInboxOptions {
  desk: DeskResponse;
  /** Muat ulang Desk; dipanggil setelah 409 agar server yang menentukan baris yang tersisa. */
  onReload?: () => void;
}

/**
 * Keadaan inbox yang dibagi desktop (`Inbox`) dan mobile (`MobileInbox`): filter, baris yang sudah diabaikan,
 * hitungan chip, dan **Abaikan** (`POST /events/{id}/dismiss` dengan `Idempotency-Key`; baris hilang hanya setelah
 * server menjawab 200, 409 = sudah diabaikan). `dismiss` mengembalikan `true` bila baris sudah hilang dari layar.
 */
export function useInbox({ desk, onReload }: UseInboxOptions) {
  const [filter, setFilter] = useState<EventFilter>("semua");
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const [errors, setErrors] = useState<Readonly<Record<string, string>>>({});
  const [announce, setAnnounce] = useState("");
  // Satu kunci per peristiwa: mengulang Abaikan setelah timeout memakai kunci yang sama.
  const keys = useRef(new Map<string, string>());
  const region = useRef<HTMLDivElement>(null);

  const visible = useMemo(() => desk.inbox.filter((e) => !gone.has(e.id)), [desk.inbox, gone]);
  const shown = useMemo(() => visible.filter((e) => filter === "semua" || e.group === filter), [visible, filter]);
  const days = useMemo(() => {
    const out: Array<{ day: string; events: ResearchEvent[] }> = [];
    for (const e of shown) {
      const day = dayOf(e);
      const last = out[out.length - 1];
      if (last && last.day === day) last.events.push(e);
      else out.push({ day, events: [e] });
    }
    return out;
  }, [shown]);
  // Hitungan dari backend dikurangi baris yang sudah diabaikan di layar ini, sampai Desk dimuat ulang.
  const countOf = (group: EventFilter, backend: number | null) => {
    const removed = desk.inbox.filter((e) => gone.has(e.id) && (group === "semua" || e.group === group)).length;
    return backend === null ? null : Math.max(0, backend - removed);
  };

  const dismiss = async (e: ResearchEvent): Promise<boolean> => {
    setPending((p) => new Set(p).add(e.id));
    setErrors((m) => Object.fromEntries(Object.entries(m).filter(([id]) => id !== e.id)));
    const key = keys.current.get(e.id) ?? newIdempotencyKey();
    keys.current.set(e.id, key);
    let conflict = false;
    try {
      await dismissEvent(e.id, { idempotencyKey: key });
    } catch (err) {
      // 409: sudah diabaikan, atau kunci yang sama masih diproses. Baris hilang dulu, lalu Desk dimuat ulang
      // supaya server yang menentukan apakah peristiwa itu masih ada.
      conflict = err instanceof ApiError && err.status === 409;
      if (!conflict) {
        setErrors((m) => ({ ...m, [e.id]: "Gagal mengabaikan, coba lagi" }));
        setPending((p) => {
          const next = new Set(p);
          next.delete(e.id);
          return next;
        });
        return false;
      }
    }
    // Fokus pindah ke Buka baris berikutnya (atau sebelumnya), hanya bila fokus ada di baris ini atau hilang.
    const li = region.current?.querySelector<HTMLElement>(`[data-event-id="${CSS.escape(e.id)}"]`);
    const active = document.activeElement;
    const focusHere = !active || active === document.body || (li !== null && li !== undefined && li.contains(active));
    const sibling = li?.nextElementSibling ?? li?.previousElementSibling;
    const target = sibling?.querySelector<HTMLElement>("button:not([aria-disabled])") ?? region.current;
    if (focusHere) target?.focus();
    setGone((g) => new Set(g).add(e.id));
    setPending((p) => {
      const next = new Set(p);
      next.delete(e.id);
      return next;
    });
    // Kalimat peristiwa ikut dibacakan, jadi dua pengabaian berturut-turut pada ticker yang sama tetap terdengar.
    setAnnounce(`${e.ticker ?? "Semua emiten"}: ${e.text ?? "peristiwa"} diabaikan`);
    if (conflict) onReload?.();
    return true;
  };

  return { filter, setFilter, shown, days, countOf, dismiss, pending, errors, announce, region };
}

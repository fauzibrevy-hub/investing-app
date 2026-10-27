const timeFormat = new Intl.DateTimeFormat("id-ID", { hour: "2-digit", minute: "2-digit" });
const dayFormat = new Intl.DateTimeFormat("id-ID", { day: "numeric", month: "short" });

/** "14.32" untuk hari ini, "19 Okt 14.32" untuk hari lain. Dipakai stempel waktu data. */
export function formatDataTime(date: Date, now: Date = new Date()): string {
  const sameDay = date.toDateString() === now.toDateString();
  return sameDay ? timeFormat.format(date) : `${dayFormat.format(date)} ${timeFormat.format(date)}`;
}

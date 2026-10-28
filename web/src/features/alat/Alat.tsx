import { useDesk } from "../research/api";
import { DeskFooter } from "../research/desk/DeskFooter";
import { PAGE } from "../riset/ui";
import { ToolRow } from "./ToolRow";

const TOOLS: Array<{ to: string; name: string; purpose: string }> = [
  { to: "/alat/bandingkan", name: "Bandingkan", purpose: "Satu paket, beberapa emiten berdampingan" },
  { to: "/alat/kalibrasi", name: "Kalibrasi", purpose: "Klaim dan level Anda, dan apa yang terjadi sesudahnya" },
  { to: "/alat/paket", name: "Paket & aturan", purpose: "Metrik per industri, sumber pendorong, aturan" },
];

/**
 * Alat `#/alat` (design.md §3.5): judul, tiga baris alat, footer. Daftar tetap; satu-satunya data adalah umur data di
 * footer dari `GET /desk`, dan bila gagal footer hanya memuat "Bukan saran investasi".
 */
export function Alat() {
  const desk = useDesk();
  return (
    <div className={PAGE}>
      <header className="flex min-h-11 items-center">
        <h1 className="text-title text-text">Alat</h1>
      </header>
      <ul aria-label="Alat">
        {TOOLS.map((t) => (
          <li key={t.to} className="border-b border-border-hairline last:border-b-0">
            <ToolRow {...t} />
          </li>
        ))}
      </ul>
      <DeskFooter ages={desk.data?.ages} />
    </div>
  );
}

export default Alat;

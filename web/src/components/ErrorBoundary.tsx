import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  /** Saat berubah (mis. pindah rute), error dibersihkan sehingga navigasi memulihkan layar. */
  resetKey?: string;
}

/** Membatasi error render pada konten layar; shell dan navigasi bawah tetap hidup. */
export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Layar gagal dirender", error, info.componentStack);
  }

  componentDidUpdate(prev: Props) {
    if (this.state.error && prev.resetKey !== this.props.resetKey) this.setState({ error: null });
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div role="alert" className="mx-auto flex max-w-sm flex-col items-center gap-3 p-8 text-center">
        <p className="text-base font-semibold">Layar ini gagal dimuat.</p>
        <p className="text-sm opacity-70">Terjadi kesalahan tak terduga. Muat ulang aplikasi, atau pilih menu lain di bawah.</p>
        <button type="button" className="min-h-11 rounded-xl border px-4 font-medium" onClick={() => window.location.reload()}>
          Muat ulang
        </button>
      </div>
    );
  }
}

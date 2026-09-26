import { useEffect, useState, type ReactNode } from "react";
import { fileUrl, PlaceService } from "../api/services";
import type { Place, PlaceCandidate } from "../api/types";
import { CATEGORY, STATUS } from "../lib/labels";

export function Thumb({ path, alt = "", className = "", fallback }: { path?: string | null; alt?: string; className?: string; fallback?: ReactNode }) {
  const src = fileUrl(path);
  const [failed, setFailed] = useState(false);
  if (!src || failed) return <div className={`thumb thumb-empty ${className}`}>{fallback ?? "🖼️"}</div>;
  return <img className={`thumb ${className}`} src={src} alt={alt} loading="lazy" onError={() => setFailed(true)} />;
}

export function Pill({ tone = "muted", children, title }: { tone?: string; children: ReactNode; title?: string }) {
  return <span className={`pill pill-${tone}`} title={title}>{children}</span>;
}

export function CategoryChip({ category }: { category: Place["category"] }) {
  const c = CATEGORY[category] ?? CATEGORY.other;
  return <span className="chip" style={{ borderColor: c.color, color: c.color }}>{c.emoji} {c.label}</span>;
}

export function PlaceLine({ place }: { place: Place }) {
  return (
    <span className="muted">
      <Flag code={place.countryCode} name={place.country} /> {[place.city, place.country].filter(Boolean).join(", ") || "Unknown location"}
    </span>
  );
}

export function StatusBadge({ place }: { place: Place }) {
  const s = STATUS[place.personalStatus];
  return <span title={s.label}>{s.emoji}</span>;
}

export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className={`modal ${wide ? "modal-wide" : ""}`} onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3>{title}</h3>
          <button className="icon-btn" onClick={onClose} aria-label="Close">✕</button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}

export function ErrorNote({ error, onClose }: { error?: string; onClose?: () => void }) {
  if (!error) return null;
  return (
    <div className="error-note">
      <span>{error}</span>
      {onClose && <button className="icon-btn" onClick={onClose}>✕</button>}
    </div>
  );
}

export function Empty({ icon, title, children }: { icon: string; title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {children && <div className="muted">{children}</div>}
    </div>
  );
}

/** Map provider search dialog used for corrections. Coordinates come only from the provider. */
export function PlaceSearchDialog({ title = "Find the place", initialQuery = "", onPick, onClose }: {
  title?: string; initialQuery?: string; onPick: (c: PlaceCandidate) => void; onClose: () => void;
}) {
  const [query, setQuery] = useState(initialQuery);
  const [results, setResults] = useState<PlaceCandidate[]>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  const search = async () => {
    if (!query.trim()) return;
    setBusy(true);
    setError(undefined);
    try {
      setResults(await PlaceService.searchMap(query.trim()));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => { if (initialQuery) void search(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);

  return (
    <Modal title={title} onClose={onClose}>
      <form className="row" onSubmit={(e) => { e.preventDefault(); void search(); }}>
        <input autoFocus className="grow" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="e.g. Blue Lagoon, Iceland" />
        <button className="btn primary" disabled={busy}>{busy ? "Searching…" : "Search"}</button>
      </form>
      <ErrorNote error={error} />
      <div className="candidate-list">
        {results?.length === 0 && <p className="muted">No results. Try adding the city or country.</p>}
        {results?.map((c, i) => <CandidateRow key={i} candidate={c} onPick={() => onPick(c)} />)}
      </div>
    </Modal>
  );
}

export function CandidateRow({ candidate, onPick, action = "Choose" }: { candidate: PlaceCandidate; onPick: () => void; action?: string }) {
  return (
    <div className="candidate">
      <div>
        <div className="strong"><Flag code={candidate.countryCode} name={candidate.country} /> {candidate.name}</div>
        <div className="muted small">{[candidate.address, candidate.country].filter(Boolean).join(" · ")}</div>
      </div>
      <button className="btn" onClick={onPick}>{action}</button>
    </div>
  );
}

/** Country flag as a crisp SVG (flag-icons, bundled locally) — consistent everywhere, unlike emoji flags. */
export function Flag({ code, name }: { code?: string | null; name?: string | null }) {
  const label = name ?? code ?? "Unknown country";
  if (!code || !/^[a-z]{2}$/i.test(code)) return <span className="flag flag-unknown" role="img" aria-label={label} title={label} />;
  return <span className={`flag fi fi-${code.toLowerCase()}`} role="img" aria-label={label} title={label} />;
}

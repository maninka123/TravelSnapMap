import { ImageIcon, X } from "lucide-react";
import { useState, type ReactNode } from "react";
import { fileUrl } from "../api/services";
import type { Place, PlaceCandidate } from "../api/types";
import { CATEGORY } from "../lib/labels";
import { CategoryGlyph, colorOf } from "../lib/categoryIcons";
import { StatusTag } from "../lib/icons";

export { Modal } from "./ui";

export function Thumb({ path, alt = "", className = "", fallback, focus }: { path?: string | null; alt?: string; className?: string; fallback?: ReactNode; focus?: string | null }) {
  const src = fileUrl(path);
  const [failed, setFailed] = useState(false);
  if (!src || failed) return <div className={`thumb thumb-empty ${className}`} aria-hidden={!alt}>{fallback ?? <ImageIcon size={22} strokeWidth={1.6} />}</div>;
  return <img className={`thumb ${className}`} src={src} alt={alt} loading="lazy" decoding="async" draggable={false}
              onError={() => setFailed(true)} style={focus ? { objectPosition: focus } : undefined} />;
}

export function Pill({ tone = "muted", children, title }: { tone?: string; children: ReactNode; title?: string }) {
  return <span className={`pill pill-${tone}`} title={title}>{children}</span>;
}

export function CategoryChip({ category }: { category: Place["category"] }) {
  const c = CATEGORY[category] ?? CATEGORY.other;
  return <span className="chip" style={{ color: colorOf(category) }}><CategoryGlyph category={category} size={12} /><span style={{ color: "var(--text-2)" }}>{c.label}</span></span>;
}

export function PlaceLine({ place }: { place: Pick<Place, "city" | "country" | "countryCode"> }) {
  return (
    <span className="place-where">
      <Flag code={place.countryCode} name={place.country} /> <span>{[place.city, place.country].filter(Boolean).join(", ") || "Unknown location"}</span>
    </span>
  );
}

/** Kept for older call sites: the status as a compact coloured tag. */
export function StatusBadge({ place }: { place: Place }) {
  return <StatusTag status={place.personalStatus} compact />;
}

export function ErrorNote({ error, onClose }: { error?: string; onClose?: () => void }) {
  if (!error) return null;
  return (
    <div className="error-note" role="alert">
      <span>{friendlyError(error)}</span>
      {onClose && <button className="close-btn small" onClick={onClose} aria-label="Dismiss"><X size={12} /></button>}
    </div>
  );
}

/** Backend errors arrive as plain strings; trim technical prefixes so people see the useful part. */
export function friendlyError(e: string): string {
  return e.replace(/^Error:\s*/i, "").replace(/^(anyhow|rusqlite|io) error:\s*/i, "").trim();
}

export function Empty({ icon, title, children, actions }: { icon: ReactNode; title: string; children?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {children && <div className="muted">{children}</div>}
      {actions && <div className="empty-actions">{actions}</div>}
    </div>
  );
}

export function CandidateRow({ candidate, onPick, action = "Choose" }: { candidate: PlaceCandidate; onPick: () => void; action?: string }) {
  return (
    <div className="candidate">
      <div className="grow">
        <div className="strong row"><Flag code={candidate.countryCode} name={candidate.country} /> {candidate.name}</div>
        <div className="muted small truncate">{[candidate.address, candidate.city, candidate.country].filter(Boolean).join(" · ")}</div>
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

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}

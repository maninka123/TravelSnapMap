import { AlertTriangle, Images } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Place, PlaceWarning } from "../../api/types";
import { CategoryChip, Flag, Pill, Thumb } from "../../components/common";
import { CategoryBadge } from "../../lib/categoryIcons";
import { StatusTag } from "../../lib/icons";
import { areaKey } from "../../lib/countryBubbles";

export type LibrarySort = "recent" | "name" | "sources";
const PAGE = 120;

/**
 * Every saved place as cards, grouped by country (biggest first); inside a country, places in the same city sit
 * together with the city itself first. Renders progressively so very large libraries stay responsive.
 */
export function Library({ places, sort, warnings, onOpen }: {
  places: Place[]; sort: LibrarySort; warnings: Record<string, PlaceWarning[]>; onOpen: (p: Place, order: Place[]) => void;
}) {
  const groups = useMemo(() => groupByCountry(places, sort), [places, sort]);
  const order = useMemo(() => groups.flatMap(([, items]) => items), [groups]);
  const [limit, setLimit] = useState(PAGE);
  useEffect(() => setLimit(PAGE), [places, sort]);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = sentinel.current;
    if (!el || limit >= order.length) return;
    const io = new IntersectionObserver((entries) => { if (entries.some((e) => e.isIntersecting)) setLimit((n) => n + PAGE); }, { rootMargin: "800px" });
    io.observe(el);
    return () => io.disconnect();
  }, [limit, order.length]);

  let budget = limit;
  return (
    <div className="library" role="region" aria-label="Library">
      {groups.map(([country, items]) => {
        if (budget <= 0) return null;
        const shown = items.slice(0, budget);
        budget -= shown.length;
        return (
          <section key={country} className="country-section" aria-label={country}>
            <h3 className="country-head"><Flag code={items[0].countryCode} name={country} /> {country}<span className="muted">{items.length}</span></h3>
            <div className="grid">
              {shown.map((p) => (
                <div key={p.id} className="tile place-card" role="button" tabIndex={0} aria-label={p.canonicalName}
                     onClick={() => onOpen(p, order)} onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(p, order); } }}>
                  <span className="place-card-status"><StatusTag status={p.personalStatus} compact /></span>
                  <Thumb path={p.heroImagePath ?? p.thumbnailPath} focus={p.heroFocus} fallback={<CategoryBadge category={p.category} size={44} />} />
                  <div className="tile-body">
                    <span className="tile-title">{p.canonicalName}</span>
                    <span className="place-where small"><span>{[p.city !== p.canonicalName ? p.city : null, p.country].filter(Boolean).join(", ") || "Unknown location"}</span></span>
                    <div className="place-card-meta">
                      <CategoryChip category={p.category} />
                      <span className="muted small">{p.sourceCount} source{p.sourceCount === 1 ? "" : "s"}</span>
                      {p.memoryCount > 0 && <span className="muted small row" style={{ gap: 3 }}><Images size={12} />{p.memoryCount}</span>}
                      {p.verification === "needsReview" && <Pill tone="warn">Not confirmed</Pill>}
                      {warnings[p.id] && <Pill tone="warn" title={warnings[p.id].map((w) => w.title).join("\n")}><AlertTriangle size={11} /> Check</Pill>}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </section>
        );
      })}
      {limit < order.length && <div ref={sentinel} className="load-more"><span className="spinner" /> Loading more places…</div>}
    </div>
  );
}

export function groupByCountry(places: Place[], sort: LibrarySort): [string, Place[]][] {
  const sorted = [...places].sort((a, b) =>
    sort === "name" ? a.canonicalName.localeCompare(b.canonicalName)
      : sort === "sources" ? b.sourceCount - a.sourceCount || a.canonicalName.localeCompare(b.canonicalName)
      : b.createdAt.localeCompare(a.createdAt));
  const g = new Map<string, Place[]>();
  sorted.forEach((p) => {
    const key = p.country ?? "Unknown country";
    const list = g.get(key);
    if (list) list.push(p); else g.set(key, [p]);
  });
  const isArea = (p: Place) => p.category === "city" || p.category === "region";
  return [...g.entries()]
    .sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
    .map(([country, items]) => {
      // Places in the same city sit together (biggest city first), the city itself leading its group.
      const city = (p: Place) => (areaKey(p) ?? p.canonicalName).toLowerCase();
      const perCity = new Map<string, number>();
      items.forEach((p) => perCity.set(city(p), (perCity.get(city(p)) ?? 0) + 1));
      const rank = new Map(items.map((p, i) => [p.id, i]));
      const ordered = [...items].sort((a, b) => {
        const ca = city(a), cb = city(b);
        if (ca !== cb) return (perCity.get(cb)! - perCity.get(ca)!) || ca.localeCompare(cb);
        if (isArea(a) !== isArea(b)) return isArea(a) ? -1 : 1;
        return rank.get(a.id)! - rank.get(b.id)!;
      });
      return [country, ordered] as [string, Place[]];
    });
}

import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronRight } from "lucide-react";
import { useMemo, useRef } from "react";
import type { Place } from "../../api/types";
import { Flag, Thumb } from "../../components/common";
import { CategoryBadge } from "../../lib/categoryIcons";
import { StatusIcon } from "../../lib/icons";
import { CATEGORY } from "../../lib/labels";
import { groupForList } from "./model";

type Row =
  | { type: "group"; key: string; label: string; code: string | null; kind: "country" | "city"; count: number }
  | { type: "place"; place: Place };

/**
 * The saved places beside the map. It follows the map: only what's in view (or everything, if you prefer), grouped by
 * country when several are visible and by city once you're in one country. Virtualised, so thousands stay smooth.
 */
export function PlaceList({ places, total, inViewOnly, onInViewOnly, selectedId, onPick, onHover, onGroup }: {
  places: Place[]; total: number; inViewOnly: boolean; onInViewOnly: (v: boolean) => void; selectedId?: string;
  onPick: (p: Place) => void; onHover: (id?: string) => void; onGroup: (g: { kind: "country" | "city"; key: string; label: string; code: string | null }) => void;
}) {
  const rows = useMemo<Row[]>(() => groupForList(places).flatMap((g) => [
    { type: "group" as const, key: g.key, label: g.label, code: g.code, kind: g.kind, count: g.items.length },
    ...g.items.map((place) => ({ type: "place" as const, place })),
  ]), [places]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtual = useVirtualizer({
    count: rows.length, getScrollElement: () => scrollRef.current, overscan: 12,
    estimateSize: (i) => (rows[i].type === "group" ? 34 : 58),
  });

  return (
    <aside className="place-list" aria-label="Places">
      <div className="place-list-head">
        <div className="grow">
          <span className="strong">{inViewOnly && places.length !== total ? `${places.length.toLocaleString()} in view` : `${places.length.toLocaleString()} place${places.length === 1 ? "" : "s"}`}</span>
          {inViewOnly && places.length !== total && <span className="muted small">of {total.toLocaleString()} matching</span>}
        </div>
        <label className="row small muted" title="List only the places visible on the map">
          <input type="checkbox" checked={inViewOnly} onChange={(e) => onInViewOnly(e.target.checked)} /> In view
        </label>
      </div>
      <div ref={scrollRef} className="place-list-scroll">
        {places.length === 0 ? (
          <p className="muted small" style={{ padding: "12px 8px" }}>
            {total === 0 ? "No places match your filters." : "No saved places in this part of the map. Zoom out, or untick “In view”."}
          </p>
        ) : (
          <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
            {virtual.getVirtualItems().map((item) => {
              const row = rows[item.index];
              return (
                <div key={item.key} data-index={item.index} ref={virtual.measureElement}
                     style={{ position: "absolute", top: 0, left: 0, right: 0, transform: `translateY(${item.start}px)` }}>
                  {row.type === "group" ? (
                    <div className="pl-group">
                      <button onClick={() => onGroup(row)} title={`Show ${row.label} on the map`}>
                        {row.kind === "country" && <Flag code={row.code} name={row.label} />}
                        {row.label} <span className="muted">{row.count}</span> <ChevronRight size={13} className="faint" />
                      </button>
                    </div>
                  ) : (
                    <button className={`pl-row ${selectedId === row.place.id ? "active" : ""}`} onClick={() => onPick(row.place)}
                            onMouseEnter={() => onHover(row.place.id)} onMouseLeave={() => onHover(undefined)}
                            onFocus={() => onHover(row.place.id)} onBlur={() => onHover(undefined)}>
                      <span className="pl-thumb">
                        <Thumb path={row.place.thumbnailPath ?? row.place.heroImagePath} focus={row.place.heroFocus} fallback={<CategoryBadge category={row.place.category} size={30} />} />
                      </span>
                      <span className="pl-text">
                        <span className="pl-name">{row.place.canonicalName}</span>
                        <span className="pl-sub">{[(CATEGORY[row.place.category] ?? CATEGORY.other).label, row.place.city !== row.place.canonicalName ? row.place.city : null].filter(Boolean).join(" · ")}</span>
                      </span>
                      <span className={`status-tag status-${row.place.personalStatus}`} style={{ padding: 4 }} title={row.place.personalStatus}>
                        <StatusIcon status={row.place.personalStatus} size={11} />
                      </span>
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </aside>
  );
}

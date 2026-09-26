import { useEffect, useRef, useState } from "react";
import type { PlaceCategory } from "../api/types";
import { CATEGORY } from "../lib/labels";
import { CategoryBadge, GROUPS } from "../lib/categoryIcons";

/**
 * Kind-of-place menu with the same icons as the map, grouped like the map's filters.
 * (A native <select> can't show icons.) `allowAuto` adds an "Automatic" choice with value "".
 */
export function CategoryPicker({ value, onChange, allowAuto = false, autoLabel = "Automatic" }: {
  value: PlaceCategory | ""; onChange: (c: PlaceCategory | "") => void; allowAuto?: boolean; autoLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", away); document.removeEventListener("keydown", esc); };
  }, [open]);

  const pick = (c: PlaceCategory | "") => { onChange(c); setOpen(false); };
  const label = value ? CATEGORY[value]?.label : autoLabel;

  return (
    <div className="cat-picker" ref={ref}>
      <button type="button" className="cat-picker-btn" aria-haspopup="listbox" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        {value ? <CategoryBadge category={value} size={20} /> : <span className="cat-picker-auto">✦</span>}
        <span className="grow">{label}</span>
        <span className="cat-picker-chevron">▾</span>
      </button>
      {open && (
        <div className="cat-picker-menu" role="listbox">
          {allowAuto && (
            <button type="button" role="option" aria-selected={value === ""} className={`cat-option ${value === "" ? "active" : ""}`} onClick={() => pick("")}>
              <span className="cat-picker-auto">✦</span><span className="grow">{autoLabel}</span>{value === "" && <span className="cat-check">✓</span>}
            </button>
          )}
          {Object.entries(GROUPS).map(([key, group]) => (
            <div key={key} className="cat-group">
              <h5>{group.label}</h5>
              {group.members.map((c) => (
                <button key={c} type="button" role="option" aria-selected={value === c} className={`cat-option ${value === c ? "active" : ""}`} onClick={() => pick(c)}>
                  <CategoryBadge category={c} size={22} />
                  <span className="grow">{CATEGORY[c]?.label ?? c}</span>
                  {value === c && <span className="cat-check">✓</span>}
                </button>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { fileUrl, PlaceService } from "../../api/services";
import type { Place } from "../../api/types";
import { CategoryBadge } from "../../lib/categoryIcons";
import { useAction } from "../../lib/nav";

const parse = (focus: string | null | undefined): [number, number] => {
  const m = focus?.match(/([\d.]+)%\s+([\d.]+)%/);
  return m ? [Number(m[1]), Number(m[2])] : [50, 50];
};
const clamp = (v: number) => Math.min(100, Math.max(0, v));

/**
 * The place's cover photo. Press and drag to choose which part shows in the frame (nothing is cropped);
 * the position is saved for the place and used on every card. Double-click to centre it again.
 */
export function HeroPhoto({ place, path }: { place: Place; path: string | null }) {
  const action = useAction();
  const img = useRef<HTMLImageElement>(null);
  const [focus, setFocus] = useState<[number, number]>(() => parse(place.heroFocus));
  const [dragging, setDragging] = useState(false);
  const [failed, setFailed] = useState(false);
  const drag = useRef<{ x: number; y: number; start: [number, number]; overX: number; overY: number } | null>(null);
  useEffect(() => setFocus(parse(place.heroFocus)), [place.id, place.heroFocus]);

  const src = fileUrl(path);
  if (!src || failed) return <div className="hero"><div className="thumb thumb-empty"><CategoryBadge category={place.category} size={72} /></div></div>;

  const save = (f: [number, number]) =>
    void action.run(() => PlaceService.update(place.id, "heroFocus", `${f[0].toFixed(1)}% ${f[1].toFixed(1)}%`));

  const onDown = (e: React.PointerEvent<HTMLImageElement>) => {
    const el = img.current;
    if (!el || !el.naturalWidth) return;
    // How far the photo overflows its frame (object-fit: cover): that's how far it can move on each axis.
    const box = el.getBoundingClientRect();
    const scale = Math.max(box.width / el.naturalWidth, box.height / el.naturalHeight);
    drag.current = { x: e.clientX, y: e.clientY, start: focus, overX: el.naturalWidth * scale - box.width, overY: el.naturalHeight * scale - box.height };
    el.setPointerCapture(e.pointerId);
    setDragging(true);
  };
  const onMove = (e: React.PointerEvent<HTMLImageElement>) => {
    const d = drag.current;
    if (!d) return;
    // 1:1 with the pointer: dragging right reveals more of the left side.
    const x = d.overX > 1 ? clamp(d.start[0] - ((e.clientX - d.x) / d.overX) * 100) : d.start[0];
    const y = d.overY > 1 ? clamp(d.start[1] - ((e.clientY - d.y) / d.overY) * 100) : d.start[1];
    setFocus([x, y]);
  };
  const onUp = () => {
    if (!drag.current) return;
    const moved = focus[0] !== drag.current.start[0] || focus[1] !== drag.current.start[1];
    drag.current = null;
    setDragging(false);
    if (moved) save(focus);
  };

  return (
    <div className={`hero hero-draggable ${dragging ? "dragging" : ""}`} title="Drag to choose what shows · double-click to centre">
      <img ref={img} className="thumb" src={src} alt="" draggable={false} onError={() => setFailed(true)}
           style={{ objectPosition: `${focus[0]}% ${focus[1]}%` }}
           onPointerDown={onDown} onPointerMove={onMove} onPointerUp={onUp} onPointerCancel={onUp}
           onDoubleClick={() => { setFocus([50, 50]); save([50, 50]); }} />
      <span className="hero-hint">✥ Drag to reposition</span>
    </div>
  );
}

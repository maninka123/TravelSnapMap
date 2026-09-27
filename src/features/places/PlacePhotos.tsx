import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { PlaceService } from "../../api/services";
import type { Place, PlaceImage, Reel, Screenshot } from "../../api/types";
import { formatDate, SOURCE } from "../../lib/labels";
import { ErrorNote, Modal, Pill, Thumb } from "../../components/common";
import { useAction, useNav } from "../../lib/nav";

const IMAGE_EXT = ["jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "tif", "tiff", "bmp"];
const isImagePath = (p: string) => IMAGE_EXT.includes(p.split(".").pop()?.toLowerCase() ?? "");

/**
 * One grid for everything behind a place: its screenshots and Reels (the evidence) and your own photos — all the
 * same size, ending with a dashed tile to add more (drop files, paste ⌘V, or click). Hover any card to make it
 * the cover or remove it (a screenshot/Reel moves to Not travel; the place stays if others still support it).
 */
export function PlacePhotos({ place, images, screenshots, reels, onNotTravel }: {
  place: Place; images: PlaceImage[]; screenshots: Screenshot[]; reels: Reel[];
  onNotTravel: (kind: "screenshot" | "reel", id: string) => void;
}) {
  const nav = useNav();
  const action = useAction();
  const zone = useRef<HTMLDivElement>(null);
  const [dragOver, setDragOver] = useState(false);
  const [viewing, setViewing] = useState<PlaceImage>();
  const own = images.filter((i) => i.isAccepted && i.origin === "user");
  // Which card is the cover: your photo itself, or the screenshot/Reel a cover photo was taken from.
  const hero = images.find((i) => i.id === place.heroImageId);
  const coverOf = (kind: "screenshot" | "reel" | "photo", id: string) => !place.coverMemoryId && !!hero &&
    (kind === "photo" ? hero.id === id : kind === "screenshot" ? hero.origin !== "user" && hero.screenshotId === id : hero.origin !== "user" && hero.reelId === id);
  const sources = [
    ...reels.map((r) => ({ kind: "reel" as const, id: r.id, thumb: r.thumbnailPath, date: r.createdAt, label: `🎬 Instagram Reel${r.creator ? ` · ${r.creator}` : ""}`, review: r.status === "needsReview" })),
    ...screenshots.map((s) => ({ kind: "screenshot" as const, id: s.id, thumb: s.thumbnailPath, date: s.creationDate,
      label: `📸 ${s.sourceType && s.sourceType !== "unknown" ? SOURCE[s.sourceType] : "Screenshot"}${s.creator ? ` · ${s.creator}` : ""}`, review: s.status === "needsReview" })),
  ];
  const list = sources.map((x) => ({ type: x.kind, id: x.id }));

  // Only the place page on top reacts to paste and drops (not one hidden underneath).
  const top = nav.panels[nav.panels.length - 1];
  const active = top?.type === "place" && top.id === place.id;

  const addPaths = (paths: string[]) => {
    const images = paths.filter(isImagePath);
    if (images.length === 0) return;
    void action.run(() => PlaceService.addPhotos(place.id, images));
  };

  // Drag & drop: macOS gives the app the dropped files' paths; accept them when dropped on this area.
  useEffect(() => {
    if (!active) return;
    const inside = (x: number, y: number) => {
      const r = zone.current?.getBoundingClientRect();
      const px = x / window.devicePixelRatio, py = y / window.devicePixelRatio;
      return !!r && px >= r.left && px <= r.right && py >= r.top && py <= r.bottom;
    };
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setDragOver(inside(p.position.x, p.position.y));
      else if (p.type === "leave") setDragOver(false);
      else if (p.type === "drop") {
        setDragOver(false);
        if (inside(p.position.x, p.position.y)) addPaths(p.paths);
      }
    });
    return () => { void unlisten.then((u) => u()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, place.id]);

  // Paste an image (⌘V) anywhere on the place page, unless you're typing.
  useEffect(() => {
    if (!active) return;
    const onPaste = async (e: ClipboardEvent) => {
      if ((e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable]")) return;
      const item = [...(e.clipboardData?.items ?? [])].find((i) => i.type.startsWith("image/"));
      const file = item?.getAsFile();
      if (!file) return;
      e.preventDefault();
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      void action.run(() => PlaceService.addPhotoBytes(place.id, bytes, file.type.split("/")[1] ?? "png"));
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, place.id]);

  const choose = async () => {
    const picked = await open({ multiple: true, directory: false, title: `Add photos to ${place.canonicalName}`,
      filters: [{ name: "Images", extensions: IMAGE_EXT }] });
    if (picked) addPaths(Array.isArray(picked) ? picked : [picked]);
  };

  return (
    <>
      <div className="section">
        <h3>Photos &amp; sources</h3><span className="muted small">{sources.length + own.length}</span>
        <span className="spacer" />
        <button className="btn small" disabled={action.busy} onClick={choose}>＋ Add photos…</button>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div ref={zone} className={`photo-zone ${dragOver ? "drag-over" : ""}`}>
        <div className="photo-grid source-grid">
          {sources.map((x) => (
            <MediaCard key={`${x.kind}-${x.id}`} thumb={x.thumb} cover={coverOf(x.kind, x.id)}
              title={x.label} sub={formatDate(x.date)} review={x.review}
              onOpen={() => (x.kind === "reel" ? nav.openReel(x.id, undefined, list) : nav.openScreenshot(x.id, undefined, list))}
              onCover={() => action.run(() => PlaceService.setCoverSource(place.id, x.kind === "screenshot" ? x.id : null, x.kind === "reel" ? x.id : null))}
              removeTitle={`Move this ${x.kind === "reel" ? "Reel" : "screenshot"} to Not travel (the place stays if other sources support it)`}
              onRemove={() => onNotTravel(x.kind, x.id)} portrait isSource />
          ))}
          {own.map((img) => (
            <MediaCard key={img.id} thumb={img.imagePath} cover={coverOf("photo", img.id)}
              title={img.caption || "Your photo"} sub={formatDate(img.createdAt)}
              onOpen={() => setViewing(img)}
              onCover={() => action.run(() => PlaceService.update(place.id, "heroImageId", img.id))}
              removeTitle="Remove this photo" onRemove={() => action.run(() => PlaceService.removeImage(img.id))} portrait />
          ))}
          {/* Always-visible drop target: drag files here, paste (⌘V), or click to choose. */}
          <button className={`photo-add ${dragOver ? "drag-over" : ""}`} onClick={choose} disabled={action.busy}>
            <span className="photo-add-plus">{action.busy ? "…" : "＋"}</span>
            <span className="photo-add-title">{action.busy ? "Adding…" : dragOver ? "Drop to add" : "Drop photos here"}</span>
            <span className="photo-add-sub">or paste ⌘V · click to choose</span>
          </button>
        </div>
      </div>
      {viewing && <PhotoViewer place={place} img={viewing} onClose={() => setViewing(undefined)} />}
    </>
  );
}

/** One card: picture, a one-line label, hover buttons for cover and remove. Same size for every kind. */
function MediaCard({ thumb, title, sub, cover, review, onOpen, onCover, onRemove, removeTitle, portrait = false, isSource = false }: {
  thumb: string | null; title: string; sub: string; cover: boolean; review?: boolean; portrait?: boolean; isSource?: boolean;
  onOpen: () => void; onCover: () => void; onRemove: () => void; removeTitle: string;
}) {
  return (
    <figure className={`photo-card ${cover ? "is-cover" : ""} ${portrait ? "portrait" : ""} ${isSource ? "is-source" : ""}`} onClick={onOpen}>
      <Thumb path={thumb} />
      <div className="photo-card-actions" onClick={(e) => e.stopPropagation()}>
        {cover ? <span className="photo-chip">★ Cover</span> : <button className="photo-chip" title="Use as the place's cover" onClick={onCover}>☆ Set cover</button>}
        <button className="close-btn small photo-remove" title={removeTitle} aria-label={removeTitle} onClick={onRemove}>✕</button>
      </div>
      <figcaption>
        <span className="photo-card-title">{title}</span>
        <span className="photo-card-sub">{review ? <Pill tone="warn">Needs review</Pill> : sub}</span>
      </figcaption>
    </figure>
  );
}

/** Opened by clicking a photo: the full image, your details (saved as you go), cover and remove. */
function PhotoViewer({ place, img, onClose }: { place: Place; img: PlaceImage; onClose: () => void }) {
  const action = useAction();
  const [caption, setCaption] = useState(img.caption ?? "");
  const isCover = place.heroImageId === img.id;
  const save = () => { if (caption.trim() !== (img.caption ?? "")) void action.run(() => PlaceService.updateCaption(img.id, caption)); };
  return (
    <Modal title={place.canonicalName} onClose={() => { save(); onClose(); }} wide>
      <div className="photo-viewer">
        <Thumb path={img.imagePath} className="photo-viewer-img" />
        <div className="photo-viewer-side">
          <label className="field">
            <span className="field-label">Details</span>
            <textarea rows={5} placeholder="What's in this photo, when you took it, tips…" value={caption}
                      onChange={(e) => setCaption(e.target.value)} onBlur={save} />
          </label>
          <span className="muted small">{img.origin === "user" ? "Your photo" : "Cut from one of your screenshots"}</span>
          <ErrorNote error={action.error} onClose={action.clearError} />
          <div className="row wrap">
            {isCover ? <Pill tone="ok">★ Cover photo</Pill> : (
              <button className="btn" onClick={() => action.run(() => PlaceService.update(place.id, "heroImageId", img.id))}>☆ Set as cover</button>
            )}
            <span className="spacer" />
            <button className="btn danger" onClick={async () => { if (await action.run(() => PlaceService.removeImage(img.id)) !== undefined) onClose(); }}>Remove</button>
          </div>
        </div>
      </div>
    </Modal>
  );
}

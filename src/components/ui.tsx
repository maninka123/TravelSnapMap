// Shared UI primitives. Dialogs, menus and popovers use Radix (focus trapping, keyboard navigation, screen readers).
import * as Dialog from "@radix-ui/react-dialog";
import * as Dropdown from "@radix-ui/react-dropdown-menu";
import * as RPopover from "@radix-ui/react-popover";
import { CheckCircle2, Info, X, XCircle } from "lucide-react";
import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";

/** A focused task: dims the window, traps focus, closes on Escape. */
export function Modal({ title, description, onClose, children, wide }: {
  title: string; description?: ReactNode; onClose: () => void; children: ReactNode; wide?: boolean;
}) {
  return (
    <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-backdrop" />
        <Dialog.Content className={`modal ${wide ? "modal-wide" : ""}`}>
          <div className="modal-head">
            <Dialog.Title asChild><h3>{title}</h3></Dialog.Title>
            <Dialog.Close className="close-btn" aria-label="Close"><X size={14} strokeWidth={2.4} /></Dialog.Close>
          </div>
          {description ? <Dialog.Description className="modal-desc">{description}</Dialog.Description> : <Dialog.Description className="visually-hidden">{title}</Dialog.Description>}
          <div className="modal-body">{children}</div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** A dropdown menu anchored to its trigger. */
export function Menu({ trigger, children, align = "end" }: { trigger: ReactNode; children: ReactNode; align?: "start" | "end" | "center" }) {
  return (
    <Dropdown.Root modal={false}>
      <Dropdown.Trigger asChild>{trigger}</Dropdown.Trigger>
      <Dropdown.Portal>
        <Dropdown.Content className="menu-content" align={align} sideOffset={6} collisionPadding={10}>{children}</Dropdown.Content>
      </Dropdown.Portal>
    </Dropdown.Root>
  );
}

export function MenuItem({ icon, children, onSelect, danger, disabled, hint }: {
  icon?: ReactNode; children: ReactNode; onSelect: () => void; danger?: boolean; disabled?: boolean; hint?: ReactNode;
}) {
  return (
    <Dropdown.Item className={`menu-item ${danger ? "danger" : ""}`} disabled={disabled} onSelect={onSelect}>
      {icon}<span className="grow">{children}</span>{hint && <span className="menu-hint">{hint}</span>}
    </Dropdown.Item>
  );
}
export const MenuSeparator = () => <Dropdown.Separator className="menu-sep" />;
export const MenuLabel = ({ children }: { children: ReactNode }) => <Dropdown.Label className="menu-label">{children}</Dropdown.Label>;

/** A non-modal panel anchored to a trigger (filters, pickers). */
export function Popover({ trigger, children, open, onOpenChange, align = "start", className = "" }: {
  trigger: ReactNode; children: ReactNode; open?: boolean; onOpenChange?: (o: boolean) => void; align?: "start" | "end" | "center"; className?: string;
}) {
  return (
    <RPopover.Root open={open} onOpenChange={onOpenChange}>
      <RPopover.Trigger asChild>{trigger}</RPopover.Trigger>
      <RPopover.Portal>
        <RPopover.Content className={`popover-content ${className}`} align={align} sideOffset={8} collisionPadding={12}>{children}</RPopover.Content>
      </RPopover.Portal>
    </RPopover.Root>
  );
}

/** Segmented control (single choice). */
export function Segmented<T extends string>({ value, onChange, options, label }: {
  value: T; onChange: (v: T) => void; label: string; options: { value: T; label: ReactNode; title?: string }[];
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={value === o.value} title={o.title}
                className={value === o.value ? "active" : ""} onClick={() => onChange(o.value)}>{o.label}</button>
      ))}
    </div>
  );
}

// MARK: Toasts

type ToastTone = "ok" | "bad" | "info";
interface ToastItem { id: number; tone: ToastTone; text: ReactNode; action?: { label: string; run: () => void } }
const ToastContext = createContext<(text: ReactNode, tone?: ToastTone, action?: ToastItem["action"]) => void>(() => {});

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const next = useRef(1);
  const dismiss = useCallback((id: number) => setItems((all) => all.filter((t) => t.id !== id)), []);
  const show = useCallback((text: ReactNode, tone: ToastTone = "ok", action?: ToastItem["action"]) => {
    const id = next.current++;
    setItems((all) => [...all.slice(-2), { id, tone, text, action }]);
    window.setTimeout(() => dismiss(id), tone === "bad" ? 8000 : 4500);
  }, [dismiss]);
  return (
    <ToastContext.Provider value={show}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((t) => (
          <div key={t.id} className={`toast ${t.tone}`}>
            {t.tone === "ok" ? <CheckCircle2 size={18} /> : t.tone === "bad" ? <XCircle size={18} /> : <Info size={18} />}
            <span className="grow">{t.text}</span>
            {t.action && <button className="btn small" onClick={() => { t.action!.run(); dismiss(t.id); }}>{t.action.label}</button>}
            <button className="icon-btn" aria-label="Dismiss" onClick={() => dismiss(t.id)}><X size={14} /></button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const show = useContext(ToastContext);
  return useMemo(() => ({
    ok: (text: ReactNode, action?: ToastItem["action"]) => show(text, "ok", action),
    error: (text: ReactNode) => show(text, "bad"),
    info: (text: ReactNode, action?: ToastItem["action"]) => show(text, "info", action),
  }), [show]);
}

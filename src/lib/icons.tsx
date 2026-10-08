// UI icons (Lucide, ISC licence). Kinds of place keep their own glyph set in categoryIcons.tsx, shared with map pins.
import {
  AlertTriangle, BedDouble, Bookmark, CalendarDays, Camera, Check, CircleSlash, Clock, Compass, Heart, Lightbulb, MapPin,
  Navigation, Sparkles, Star, Ticket, Train, Utensils, type LucideIcon,
} from "lucide-react";
import type { PersonalStatus, TravelFactType } from "../api/types";
import { STATUS } from "./labels";

export const STATUS_ICON: Record<PersonalStatus, LucideIcon> = {
  wantToVisit: Heart, maybe: Bookmark, visited: Check, favourite: Star, notInterested: CircleSlash,
};

export function StatusIcon({ status, size = 14 }: { status: PersonalStatus; size?: number }) {
  const Icon = STATUS_ICON[status] ?? Heart;
  return <Icon size={size} strokeWidth={2.2} aria-hidden="true" fill={status === "wantToVisit" || status === "favourite" ? "currentColor" : "none"} />;
}

/** "♥ Want to visit" as a coloured tag. */
export function StatusTag({ status, compact = false }: { status: PersonalStatus; compact?: boolean }) {
  return (
    <span className={`status-tag status-${status}`} title={STATUS[status].label}>
      <StatusIcon status={status} size={12} />{!compact && STATUS[status].label}
    </span>
  );
}

export const FACT_ICON: Partial<Record<TravelFactType, LucideIcon>> = {
  recommendedTime: Clock, bestSeason: CalendarDays, openingHours: Clock, duration: Clock, price: Ticket, ticket: Ticket,
  reservation: CalendarDays, transportation: Train, route: Navigation, food: Utensils, photography: Camera, warning: AlertTriangle,
  accommodation: BedDouble, activity: Sparkles, nearby: MapPin, itinerary: Compass, generalTip: Lightbulb,
};

export const INFO_CARD_ICON: Record<string, LucideIcon> = {
  time: Clock, getting: Train, cost: Ticket, food: Utensils, photo: Camera, todo: Sparkles, stay: BedDouble,
  warnings: AlertTriangle, tips: Lightbulb, nearby: MapPin, itinerary: Compass,
};

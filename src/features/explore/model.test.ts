import { describe, expect, it } from "vitest";
import type { Place } from "../../api/types";
import { demoLibrary } from "../../dev/demoData";
import { withMetro } from "../../lib/metro";
import { applyFilters, groupForList, matchesFilters, scopeFor, suggest, tallies, EMPTY_FILTERS } from "./model";

const places: Place[] = withMetro(demoLibrary().places);

describe("Explore filters", () => {
  it("matches names, alternative names, city and country", () => {
    const by = (query: string) => applyFilters(places, { ...EMPTY_FILTERS, query }).map((p) => p.canonicalName);
    expect(by("skytree")).toEqual(["Tokyo Skytree"]);
    expect(by("東京スカイツリー")).toEqual(["Tokyo Skytree"]);
    expect(by("sri lanka")).toHaveLength(4);
    expect(by("zzz")).toEqual([]);
  });

  it("combines kind, status and scope", () => {
    const japanTemples = applyFilters(places, { ...EMPTY_FILTERS, groups: ["sights"], scope: { kind: "country", key: "JP", label: "Japan", code: "JP" } });
    expect(japanTemples.length).toBeGreaterThan(0);
    expect(japanTemples.every((p) => p.countryCode === "JP")).toBe(true);
    const visited = applyFilters(places, { ...EMPTY_FILTERS, statuses: ["visited"] });
    expect(visited.map((p) => p.canonicalName).sort()).toEqual(["Fushimi Inari Taisha", "Senso-ji", "Sydney Opera House"]);
  });

  it("counts kinds and statuses for filter chips", () => {
    const t = tallies(places);
    expect([...t.statuses.values()].reduce((a, b) => a + b, 0)).toBe(places.length);
    expect(t.statuses.get("visited")).toBe(3);
  });

  it("ignores scope in matchesFilters (scope is applied separately)", () => {
    const sydney = places.find((p) => p.canonicalName === "Sydney Opera House")!;
    expect(matchesFilters(sydney, { query: "opera", groups: [], statuses: [] })).toBe(true);
  });
});

describe("Explore search suggestions", () => {
  it("offers countries when nothing is typed, biggest first", () => {
    const s = suggest(places, "");
    expect(s[0]).toMatchObject({ kind: "country", key: "JP" });
    expect(s.every((x) => x.kind === "country")).toBe(true);
  });

  it("ranks prefix matches above word and substring matches", () => {
    const s = suggest(places, "ky");
    expect(s[0]).toMatchObject({ kind: "city", label: "Kyoto" });
    expect(s.some((x) => x.kind === "place" && x.label === "Kyoto")).toBe(true);
  });

  it("inside a country offers its cities, and a city keeps the country as parent", () => {
    const japan = scopeFor({ kind: "country", key: "JP", label: "Japan", sub: "", code: "JP" });
    const s = suggest(places.filter((p) => p.countryCode === "JP"), "", japan);
    const tokyo = s.find((x) => x.kind === "city" && x.label === "Tokyo");
    expect(tokyo).toBeTruthy();
    const scope = scopeFor(tokyo as Exclude<typeof tokyo, undefined | { kind: "place" }>, japan);
    expect(scope).toMatchObject({ kind: "city", key: "Tokyo", parent: { kind: "country", key: "JP" } });
  });
});

describe("List grouping", () => {
  it("groups by country when several are shown, by city inside one country", () => {
    expect(groupForList(places)[0]).toMatchObject({ kind: "country", key: "JP" });
    const japan = groupForList(places.filter((p) => p.countryCode === "JP"));
    expect(japan.every((g) => g.kind === "city")).toBe(true);
    expect(japan.reduce((n, g) => n + g.items.length, 0)).toBe(places.filter((p) => p.countryCode === "JP").length);
  });
});

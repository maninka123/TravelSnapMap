import { describe, expect, it } from "vitest";
import { dayList, moveStop, nudge, tripLength, type Stop } from "./itinerary";

const stops: Stop[] = [
  { id: "a", day: 1 }, { id: "b", day: 1 }, { id: "c", day: 2 }, { id: "d", day: null },
];
const ids = (s: Stop[]) => s.map((x) => `${x.id}${x.day ?? "-"}`).join(" ");

describe("itinerary", () => {
  it("moves a stop into another day, at the end of that day", () => {
    expect(ids(moveStop(stops, "d", 1))).toBe("a1 b1 d1 c2");
  });

  it("drops before a given stop", () => {
    expect(ids(moveStop(stops, "c", 1, "a"))).toBe("c1 a1 b1 d-");
  });

  it("keeps days in order even when dropped into an empty earlier day", () => {
    const moved = moveStop([{ id: "x", day: 3 }, { id: "y", day: null }], "y", 1);
    expect(ids(moved)).toBe("y1 x3");
  });

  it("unschedules a stop", () => {
    expect(ids(moveStop(stops, "a", null))).toBe("b1 c2 d- a-");
  });

  it("ignores unknown ids and dropping onto itself", () => {
    expect(moveStop(stops, "zzz", 1)).toBe(stops);
    expect(moveStop(stops, "a", 1, "a")).toBe(stops);
  });

  it("nudges only within the same day", () => {
    expect(ids(nudge(stops, "b", -1))).toBe("b1 a1 c2 d-");
    expect(nudge(stops, "b", 1)).toBe(stops); // c is on another day
    expect(nudge(stops, "a", -1)).toBe(stops);
  });

  it("works out the number of days from the dates", () => {
    expect(tripLength({ startDate: "2027-03-28", endDate: "2027-04-08" })).toBe(12);
    expect(tripLength({ startDate: "2027-04-08", endDate: "2027-03-28" })).toBeNull();
    expect(tripLength({ startDate: null, endDate: null })).toBeNull();
    expect(dayList(stops, { startDate: null, endDate: null })).toEqual([1, 2]);
    expect(dayList(stops, { startDate: "2027-01-01", endDate: "2027-01-04" }, 1)).toEqual([1, 2, 3, 4, 5]);
  });
});

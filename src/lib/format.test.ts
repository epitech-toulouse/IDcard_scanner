import { describe, expect, it } from "vitest";
import {
  addDays, addMinutes, fmtTime, fmtTimeSec, joinNaive, parseNaive, pct, safeFileName, schoolYear, splitNaive, startOfWeek, toNaive,
} from "./format";

describe("dates naïves", () => {
  it("aller-retour Date <-> Naive", () => {
    const s = "2026-11-17T08:05:00";
    expect(toNaive(parseNaive(s))).toBe(s);
  });
  it("join / split", () => {
    expect(joinNaive("2026-11-17", "08:05")).toBe("2026-11-17T08:05:00");
    expect(splitNaive("2026-11-17T08:05:00")).toEqual({ date: "2026-11-17", time: "08:05" });
  });
  it("arithmétique, y compris changement de mois", () => {
    expect(addMinutes("2026-11-17T23:30:00", 45)).toBe("2026-11-18T00:15:00");
    expect(addDays("2026-11-28T08:00:00", 7)).toBe("2026-12-05T08:00:00");
  });
  it("début de semaine = lundi", () => {
    expect(startOfWeek(new Date(2026, 10, 22))).toBe("2026-11-16T00:00:00"); // dimanche 22/11
    expect(startOfWeek(new Date(2026, 10, 16, 15))).toBe("2026-11-16T00:00:00");
  });
  it("année scolaire", () => {
    expect(schoolYear(new Date(2026, 8, 1))).toBe("2026-2027");
    expect(schoolYear(new Date(2027, 5, 30))).toBe("2026-2027");
  });
  it("formatage", () => {
    expect(fmtTime("2026-11-17T08:05:00")).toBe("08h05");
    expect(fmtTime(null)).toBe("");
    expect(fmtTimeSec("2026-11-17T08:05:42")).toBe("08h05:42");
    expect(fmtTimeSec("2026-11-17T08:05")).toBe("08h05:00");
    expect(fmtTimeSec(null)).toBe("");
    expect(pct(1, 3)).toBe("33 %");
    expect(pct(0, 0)).toBe("–");
    expect(safeFileName("Réseaux – B12 17/11")).toBe("Reseaux_B12_17_11");
  });
});

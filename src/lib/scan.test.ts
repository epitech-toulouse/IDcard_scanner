import { describe, expect, it } from "vitest";
import { describeScan } from "./scan";
import type { Student } from "./types";

const st = { id: 1, first_name: "Ada", last_name: "Lovelace" } as Student;

describe("describeScan", () => {
  it("présent / retard avec l'heure", () => {
    expect(describeScan({ kind: "recorded", uid: "x", student: st, status: "present", scanned_at: "2026-11-17T08:03:00" }))
      .toEqual({ cls: "present", title: "Ada LOVELACE", sub: "Présent · 08h03" });
    expect(describeScan({ kind: "recorded", uid: "x", student: st, status: "late", scanned_at: "2026-11-17T08:20:00" }).sub)
      .toBe("En retard · 08h20");
  });

  it("carte inconnue en rouge", () => {
    expect(describeScan({ kind: "unknown_card", uid: "04:AA" })).toEqual({ cls: "absent", title: "Carte inconnue", sub: "04:AA" });
  });
});
